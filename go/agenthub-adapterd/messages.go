package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"time"
)

func (rt *Runtime) messagesMux() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/v1/messages", rt.handleMessages)
	mux.HandleFunc("/v1/responses", rt.handleResponses)
	mux.HandleFunc("/v1/chat/completions", rt.handleChatCompletions)
	mux.HandleFunc("/chat/completions", rt.handleChatCompletions)
	mux.HandleFunc("/v1/models", rt.handleModels)
	mux.HandleFunc("/models", rt.handleModels)
	mux.HandleFunc("/health", rt.handleHealth)
	return mux
}

func (rt *Runtime) handleMessages(w http.ResponseWriter, r *http.Request) {
	rt.forwardSameProtocol(w, r, surfaceMessages, "/v1/messages", "This endpoint only accepts POST /v1/messages. 本机该路径只接受 POST /v1/messages.")
}

func (rt *Runtime) handleResponses(w http.ResponseWriter, r *http.Request) {
	rt.forwardSameProtocol(w, r, surfaceResponses, "/v1/responses", "This endpoint only accepts POST /v1/responses. 本机该路径只接受 POST /v1/responses。")
}

func (rt *Runtime) handleChatCompletions(w http.ResponseWriter, r *http.Request) {
	path := r.URL.Path
	msg := "This endpoint only accepts POST " + path + ". 本机该路径只接受 POST " + path + "。"
	rt.forwardSameProtocol(w, r, surfaceChatCompletions, "/v1/chat/completions", msg)
}

func (rt *Runtime) forwardSameProtocol(w http.ResponseWriter, r *http.Request, surface, upstreamPath, methodMessage string) {
	if r.RemoteAddr != "" && !isLoopbackRemote(r.RemoteAddr) {
		http.Error(w, "loopback only", http.StatusForbidden)
		return
	}
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		writeMessagesError(w, http.StatusMethodNotAllowed, "method_not_allowed", methodMessage, "invalid_request_error")
		return
	}
	if !rt.ownerServing() {
		writeMessagesError(w, http.StatusServiceUnavailable, "bridge_stopping", "Messages listener is not serving.", "invalid_request_error")
		return
	}
	got := bearerToken(r.Header.Get("Authorization"))
	edge := rt.edgeForRequest(got, surface)
	if got == "" || edge == nil {
		writeMessagesError(w, http.StatusUnauthorized, "invalid_api_key", "Invalid local bearer token.", "invalid_request_error")
		return
	}
	if !rt.tryAcquireRequestSlot() {
		w.Header().Set("Retry-After", "1")
		writeMessagesError(w, http.StatusServiceUnavailable, "route_busy", "The local route is busy. Try again shortly.", "api_error")
		return
	}
	defer rt.releaseRequestSlot()
	rt.addInFlight(1)
	defer rt.addInFlight(-1)

	body, err := readStrictRequestBody(w, r, rt.httpPolicy.IngressBodyBytes)
	if err != nil {
		if errors.Is(err, errBodyTooLarge) {
			writeMessagesError(w, http.StatusRequestEntityTooLarge, "request_too_large", "The request body is too large.", "invalid_request_error")
			return
		}
		writeMessagesError(w, http.StatusBadRequest, "invalid_request", "Unable to read request body.", "invalid_request_error")
		return
	}
	if !isJSONObject(body) {
		writeMessagesError(w, http.StatusBadRequest, "invalid_request", "The request body must be a JSON object.", "invalid_request_error")
		return
	}
	var meta struct {
		Stream bool   `json:"stream"`
		Model  string `json:"model"`
	}
	_ = json.Unmarshal(body, &meta)

	pool := edge.Pool
	if pool == nil {
		writeMessagesError(w, http.StatusServiceUnavailable, "pool_exhausted", "No eligible pool member remains.", "api_error")
		return
	}
	model := strings.TrimSpace(meta.Model)
	if model == "" {
		model = pool.FixtureModel()
	}
	if !pool.HasModel(model) {
		writeMessagesError(w, http.StatusNotFound, "model_not_found", "Unknown model.", "invalid_request_error")
		return
	}

	client := rt.upstreamClient
	if client == nil {
		client = newUpstreamHTTPClient()
	}
	excluded := make([]string, 0, 4)
	var lastStatus int
	var lastHeader http.Header
	var lastBody []byte
	var lastStream bool
	hasLast := false
	refreshUsed := false
	var retryMember *PoolMember

	for {
		if r.Context().Err() != nil {
			return
		}
		member := retryMember
		retryMember = nil
		if member == nil {
			member = pool.Pick(model, excluded, time.Now())
		}
		if member == nil {
			if hasLast {
				writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
				return
			}
			writeMessagesError(w, http.StatusServiceUnavailable, "pool_exhausted", "No eligible pool member remains.", "api_error")
			return
		}

		memberPath := upstreamPath
		memberBody := body
		memberStream := meta.Stream
		convertChatResponse := surface == surfaceResponses && member.UpstreamTransport == transportOpenAIChatCompletions
		if convertChatResponse {
			memberPath = "/v1/chat/completions"
			memberBody, memberStream, err = encodeResponsesToChat(body)
			if err != nil {
				writeMessagesError(w, http.StatusBadRequest, "invalid_request", "The Responses request cannot be represented by this route.", "invalid_request_error")
				return
			}
		}

		resp, err := doMemberMessages(r.Context(), client, member, memberPath, memberBody, memberStream)
		if err != nil {
			if r.Context().Err() != nil {
				return
			}
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			excluded = append(excluded, member.ID)
			hasLast = true
			lastStatus = http.StatusBadGateway
			lastHeader = make(http.Header)
			lastBody = safeUpstreamErrorBody()
			lastStream = false
			continue
		}
		if resp.StatusCode == http.StatusUnauthorized && !refreshUsed && member.RefreshKind != refreshNone {
			refreshUsed = true
			_ = resp.Body.Close()
			if refreshed := rt.requestOAuthRefresh(r.Context(), edge.ID, member); refreshed != nil && refreshed.member.serves(model, time.Now()) {
				pool = refreshed.pool
				retryMember = refreshed.member
				continue
			}
		}
		if resp.StatusCode >= 300 {
			_ = resp.Body.Close()
			class := classifyHTTP(resp.StatusCode)
			if shouldFailover(class, false) {
				pool.ReportFailure(member.ID, model, class, parseRetryAfter(resp.Header.Get("Retry-After")), time.Now())
				excluded = append(excluded, member.ID)
				hasLast = true
				lastStatus = resp.StatusCode
				lastHeader = safeConvertedHeaders(resp.Header)
				lastBody = safeUpstreamErrorBody()
				lastStream = false
				continue
			}
			writeSafeUpstreamResponse(w, resp.StatusCode, resp.Header)
			return
		}

		if memberStream {
			if convertChatResponse {
				outcome := dispatchConvertedChatStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast, rt.httpPolicy)
				lastStream = false
				if outcome == dispatchContinue {
					continue
				}
				return
			}
			outcome := dispatchMemberStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast, rt.httpPolicy)
			lastStream = true
			if outcome == dispatchContinue {
				continue
			}
			return
		}

		respBody, readErr := readBoundedResponseBody(
			r.Context(),
			resp,
			rt.httpPolicy.NonStreamBodyBytes,
			rt.httpPolicy.NonStreamIdleTimeout,
			rt.httpPolicy.NonStreamTotalTimeout,
		)
		_ = resp.Body.Close()
		if readErr != nil {
			if r.Context().Err() != nil {
				return
			}
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			excluded = append(excluded, member.ID)
			hasLast = true
			lastStatus = http.StatusBadGateway
			lastHeader = make(http.Header)
			lastBody = safeUpstreamErrorBody()
			lastStream = false
			continue
		}
		if !isJSONMediaType(resp.Header.Get("Content-Type")) || !isJSONObject(respBody) {
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			excluded = append(excluded, member.ID)
			hasLast = true
			lastStatus = http.StatusBadGateway
			lastHeader = make(http.Header)
			lastBody = safeUpstreamErrorBody()
			lastStream = false
			continue
		}
		if convertChatResponse {
			translated, translateErr := translateChatJSONToResponses(respBody)
			if translateErr != nil || int64(len(translated)) > rt.httpPolicy.NonStreamBodyBytes {
				pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
				excluded = append(excluded, member.ID)
				hasLast = true
				lastStatus = http.StatusBadGateway
				lastHeader = make(http.Header)
				lastBody = convertedResponsesErrorBody()
				lastStream = false
				continue
			}
			respBody = translated
			resp.Header.Set("Content-Type", "application/json")
		}
		pool.ReportSuccess(member.ID)
		writeClientResponse(w, resp.StatusCode, resp.Header, respBody, false)
		return
	}
}

func dispatchConvertedChatStream(
	w http.ResponseWriter,
	r *http.Request,
	pool *Pool,
	member *PoolMember,
	model string,
	resp *http.Response,
	excluded *[]string,
	lastStatus *int,
	lastHeader *http.Header,
	lastBody *[]byte,
	hasLast *bool,
	policy routeHTTPSafetyPolicy,
) int {
	defer resp.Body.Close()
	class := classifyHTTP(resp.StatusCode)
	if shouldFailover(class, false) {
		pool.ReportFailure(member.ID, model, class, parseRetryAfter(resp.Header.Get("Retry-After")), time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = resp.StatusCode
		*lastHeader = safeConvertedHeaders(resp.Header)
		*lastBody = convertedResponsesErrorBody()
		return dispatchContinue
	}
	if resp.StatusCode >= 400 {
		writeClientResponse(w, resp.StatusCode, safeConvertedHeaders(resp.Header), convertedResponsesErrorBody(), false)
		return dispatchDone
	}
	if !isEventStreamMediaType(resp.Header.Get("Content-Type")) {
		pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = http.StatusBadGateway
		*lastHeader = make(http.Header)
		*lastBody = safeUpstreamErrorBody()
		return dispatchContinue
	}

	translator := newChatToResponsesSSE(model)
	streamCtx, cancelStream := context.WithCancel(r.Context())
	defer cancelStream()
	lines := scanConvertedSSELines(streamCtx, resp.Body, policy.SSEBodyBytes)
	idle := time.NewTimer(policy.SSEIdleTimeout)
	defer idle.Stop()
	dataLines := make([]string, 0, 2)
	committed := false
	upstreamBytes := 0
	outputBytes := 0
	failStream := func() int {
		if committed {
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			failure := translator.fail()
			if len(failure) > 0 {
				refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
				_, _ = w.Write(failure)
				if flusher, ok := w.(http.Flusher); ok {
					flusher.Flush()
				}
			}
			return dispatchDone
		}
		pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = http.StatusBadGateway
		*lastHeader = make(http.Header)
		*lastBody = convertedResponsesErrorBody()
		return dispatchContinue
	}
	for {
		select {
		case <-r.Context().Done():
			return dispatchDone
		case <-idle.C:
			_ = resp.Body.Close()
			return failStream()
		case item, ok := <-lines:
			if !ok || item.err != nil {
				return failStream()
			}
			if !idle.Stop() {
				select {
				case <-idle.C:
				default:
				}
			}
			idle.Reset(policy.SSEIdleTimeout)
			upstreamBytes += len(item.line) + 1
			if int64(upstreamBytes) > policy.SSEBodyBytes {
				_ = resp.Body.Close()
				return failStream()
			}
			line := strings.TrimSuffix(item.line, "\r")
			if line == "" && len(dataLines) > 0 {
				translated, done, translateErr := translator.consume([]byte(strings.Join(dataLines, "\n")))
				dataLines = dataLines[:0]
				if translateErr != nil {
					return failStream()
				}
				if len(translated) > 0 {
					if int64(outputBytes+len(translated)) > policy.SSEBodyBytes {
						_ = resp.Body.Close()
						return failStream()
					}
					if !committed {
						committed = true
						pool.ReportSuccess(member.ID)
						refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
						setSafeSuccessHeaders(w.Header(), true)
						w.WriteHeader(http.StatusOK)
					}
					refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
					if _, writeErr := w.Write(translated); writeErr != nil {
						return dispatchDone
					}
					outputBytes += len(translated)
					if flusher, ok := w.(http.Flusher); ok {
						flusher.Flush()
					}
				}
				if done {
					return dispatchDone
				}
			} else if strings.HasPrefix(line, "data:") {
				dataLines = append(dataLines, strings.TrimSpace(strings.TrimPrefix(line, "data:")))
			}
		}
	}
}

type convertedSSELine struct {
	line string
	err  error
}

func scanConvertedSSELines(ctx context.Context, body io.Reader, maxBytes int64) <-chan convertedSSELine {
	lines := make(chan convertedSSELine, 1)
	go func() {
		defer close(lines)
		scanner := bufio.NewScanner(body)
		maxToken := int(maxBytes + 1)
		if maxToken < 64*1024 {
			maxToken = 64 * 1024
		}
		scanner.Buffer(make([]byte, 64*1024), maxToken)
		for scanner.Scan() {
			select {
			case lines <- convertedSSELine{line: scanner.Text()}:
			case <-ctx.Done():
				return
			}
		}
		err := scanner.Err()
		if err == nil {
			err = io.EOF
		}
		select {
		case lines <- convertedSSELine{err: err}:
		case <-ctx.Done():
		}
	}()
	return lines
}

func convertedResponsesErrorBody() []byte {
	return safeUpstreamErrorBody()
}

func safeConvertedHeaders(source http.Header) http.Header {
	header := make(http.Header)
	setSafeErrorHeaders(header, source)
	return header
}

const (
	dispatchDone     = 0
	dispatchContinue = 1
)

func dispatchMemberStream(
	w http.ResponseWriter,
	r *http.Request,
	pool *Pool,
	member *PoolMember,
	model string,
	resp *http.Response,
	excluded *[]string,
	lastStatus *int,
	lastHeader *http.Header,
	lastBody *[]byte,
	hasLast *bool,
	policy routeHTTPSafetyPolicy,
) int {
	defer resp.Body.Close()
	class := classifyHTTP(resp.StatusCode)
	if shouldFailover(class, false) {
		pool.ReportFailure(member.ID, model, class, parseRetryAfter(resp.Header.Get("Retry-After")), time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = resp.StatusCode
		*lastHeader = safeConvertedHeaders(resp.Header)
		*lastBody = safeUpstreamErrorBody()
		return dispatchContinue
	}
	if resp.StatusCode >= 300 {
		writeSafeUpstreamResponse(w, resp.StatusCode, resp.Header)
		return dispatchDone
	}
	committed, relayErr := relayBoundedSSE(w, r, resp, policy)
	if relayErr == nil {
		pool.ReportSuccess(member.ID)
		return dispatchDone
	}
	if r.Context().Err() != nil || errors.Is(relayErr, errDownstreamWrite) {
		return dispatchDone
	}
	pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
	if committed {
		writeSafeSSETermination(w)
		return dispatchDone
	}
	*excluded = append(*excluded, member.ID)
	*hasLast = true
	*lastStatus = http.StatusBadGateway
	*lastHeader = make(http.Header)
	*lastBody = safeUpstreamErrorBody()
	return dispatchContinue
}

func doMemberMessages(ctx context.Context, client *http.Client, member *PoolMember, upstreamPath string, body []byte, stream bool) (*http.Response, error) {
	upstream := joinUpstreamPath(member.UpstreamBaseURL, upstreamPath)
	if err := validateFinalUpstreamURL(upstream, member.UpstreamTransport); err != nil {
		return nil, err
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, upstream, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	if stream {
		req.Header.Set("Accept", "text/event-stream")
	}
	if member.UpstreamKey != "" && member.UpstreamAuth == authAPIKey {
		req.Header.Set("X-API-Key", member.UpstreamKey)
		if member.UpstreamTransport == transportAnthropicMessages {
			req.Header.Set("Anthropic-Version", "2023-06-01")
		}
	} else if member.UpstreamKey != "" {
		req.Header.Set("Authorization", "Bearer "+member.UpstreamKey)
	}
	return client.Do(req)
}

func joinUpstreamPath(base, endpoint string) string {
	base = strings.TrimRight(base, "/")
	if strings.HasSuffix(base, "/v1") && strings.HasPrefix(endpoint, "/v1/") {
		return base + strings.TrimPrefix(endpoint, "/v1")
	}
	return base + "/" + strings.TrimLeft(endpoint, "/")
}

func (rt *Runtime) handleModels(w http.ResponseWriter, r *http.Request) {
	if r.RemoteAddr != "" && !isLoopbackRemote(r.RemoteAddr) {
		http.Error(w, "loopback only", http.StatusForbidden)
		return
	}
	if r.Method != http.MethodGet {
		w.Header().Set("Allow", "GET")
		writeMessagesError(w, http.StatusMethodNotAllowed, "method_not_allowed", "This endpoint only accepts GET.", "invalid_request_error")
		return
	}
	if !rt.ownerServing() {
		writeMessagesError(w, http.StatusServiceUnavailable, "bridge_stopping", "Messages listener is not serving.", "invalid_request_error")
		return
	}
	got := bearerToken(r.Header.Get("Authorization"))
	edge := rt.edgeForIngress(got)
	if got == "" || edge == nil {
		writeMessagesError(w, http.StatusUnauthorized, "invalid_api_key", "Invalid local bearer token.", "invalid_request_error")
		return
	}
	ids := []string{}
	if edge.Pool != nil {
		ids = edge.Pool.ModelIDs()
	}
	data := make([]map[string]string, 0, len(ids))
	for _, id := range ids {
		data = append(data, map[string]string{"id": id, "object": "model"})
	}
	w.Header().Set("Content-Type", "application/json")
	refreshDownstreamWriteDeadline(w, rt.httpPolicy.DownstreamWriteTimeout)
	_ = json.NewEncoder(w).Encode(map[string]any{
		"object": "list",
		"data":   data,
	})
}

func (rt *Runtime) handleHealth(w http.ResponseWriter, r *http.Request) {
	if r.RemoteAddr != "" && !isLoopbackRemote(r.RemoteAddr) {
		http.Error(w, "loopback only", http.StatusForbidden)
		return
	}
	if r.Method != http.MethodGet {
		w.Header().Set("Allow", "GET")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	if !rt.ownerServing() {
		writeMessagesError(w, http.StatusServiceUnavailable, "bridge_stopping", "Messages listener is not serving.", "invalid_request_error")
		return
	}
	got := bearerToken(r.Header.Get("Authorization"))
	edge := rt.edgeForIngress(got)
	if got == "" || edge == nil {
		writeMessagesError(w, http.StatusUnauthorized, "invalid_api_key", "Invalid local bearer token.", "invalid_request_error")
		return
	}
	rt.mu.Lock()
	listenReady := rt.listenReady
	rt.mu.Unlock()
	memberCount := 0
	healthyCount := 0
	if edge.Pool != nil {
		snap := edge.Pool.Snapshot(time.Now())
		memberCount = snap.MemberCount
		healthyCount = snap.HealthyMemberCount
	}
	w.Header().Set("Content-Type", "application/json")
	refreshDownstreamWriteDeadline(w, rt.httpPolicy.DownstreamWriteTimeout)
	_ = json.NewEncoder(w).Encode(map[string]any{
		"listen_ready":         listenReady,
		"member_count":         memberCount,
		"healthy_member_count": healthyCount,
	})
}

func bearerToken(header string) string {
	const prefix = "Bearer "
	if strings.HasPrefix(header, prefix) {
		return strings.TrimSpace(header[len(prefix):])
	}
	return ""
}

func writeMessagesError(w http.ResponseWriter, status int, code, message, typ string) {
	refreshDownstreamWriteDeadline(w, defaultRouteHTTPSafetyPolicy.DownstreamWriteTimeout)
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(map[string]any{
		"error": map[string]string{
			"code":    code,
			"message": message,
			"type":    typ,
		},
	})
}

func writeClientResponse(w http.ResponseWriter, status int, header http.Header, body []byte, stream bool) {
	refreshDownstreamWriteDeadline(w, defaultRouteHTTPSafetyPolicy.DownstreamWriteTimeout)
	if status >= 300 {
		setSafeErrorHeaders(w.Header(), header)
		body = safeUpstreamErrorBody()
		stream = false
	} else {
		setSafeSuccessHeaders(w.Header(), stream)
	}
	w.WriteHeader(status)
	if len(body) > 0 {
		_, _ = w.Write(body)
	}
}
