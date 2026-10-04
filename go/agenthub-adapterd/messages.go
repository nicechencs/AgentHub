package main

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
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

	body, err := io.ReadAll(io.LimitReader(r.Body, 8<<20))
	if err != nil {
		writeMessagesError(w, http.StatusBadRequest, "invalid_request", "Unable to read request body.", "invalid_request_error")
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

	rt.addInFlight(1)
	defer rt.addInFlight(-1)

	client := newUpstreamHTTPClient()
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
			continue
		}
		if resp.StatusCode == http.StatusUnauthorized && !refreshUsed && member.RefreshKind != refreshNone {
			refreshUsed = true
			if refreshed := rt.requestOAuthRefresh(r.Context(), edge.ID, member); refreshed != nil && refreshed.member.serves(model, time.Now()) {
				_ = resp.Body.Close()
				pool = refreshed.pool
				retryMember = refreshed.member
				continue
			}
		}

		if memberStream {
			if convertChatResponse {
				outcome := dispatchConvertedChatStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast)
				lastStream = false
				if outcome == dispatchContinue {
					continue
				}
				return
			}
			outcome := dispatchMemberStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast)
			lastStream = true
			if outcome == dispatchContinue {
				continue
			}
			return
		}

		respBody, readErr := io.ReadAll(resp.Body)
		_ = resp.Body.Close()
		if readErr != nil {
			if r.Context().Err() != nil {
				return
			}
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			excluded = append(excluded, member.ID)
			continue
		}
		class := classifyHTTP(resp.StatusCode)
		if shouldFailover(class, false) {
			pool.ReportFailure(member.ID, model, class, parseRetryAfter(resp.Header.Get("Retry-After")), time.Now())
			excluded = append(excluded, member.ID)
			hasLast = true
			lastStatus = resp.StatusCode
			lastHeader = resp.Header.Clone()
			if convertChatResponse {
				lastBody = convertedResponsesErrorBody()
			} else {
				lastBody = respBody
			}
			lastStream = false
			continue
		}
		if convertChatResponse && resp.StatusCode >= 400 {
			writeClientResponse(w, resp.StatusCode, safeConvertedHeaders(resp.Header), convertedResponsesErrorBody(), false)
			return
		}
		if convertChatResponse {
			translated, translateErr := translateChatJSONToResponses(respBody)
			if translateErr != nil {
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
		if resp.StatusCode < 400 {
			pool.ReportSuccess(member.ID)
		}
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

	translator := newChatToResponsesSSE(model)
	streamCtx, cancelStream := context.WithCancel(r.Context())
	defer cancelStream()
	lines := scanConvertedSSELines(streamCtx, resp.Body)
	idle := time.NewTimer(convertedSSEIdleTimeout)
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
			idle.Reset(convertedSSEIdleTimeout)
			upstreamBytes += len(item.line) + 1
			if upstreamBytes > convertedSSELimitBytes {
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
					if outputBytes+len(translated) > convertedSSELimitBytes {
						_ = resp.Body.Close()
						return failStream()
					}
					if !committed {
						committed = true
						pool.ReportSuccess(member.ID)
						copyUpstreamHeaders(w.Header(), resp.Header)
						w.Header().Set("Content-Type", "text/event-stream")
						w.WriteHeader(http.StatusOK)
					}
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

const (
	convertedSSELimitBytes  = 32 * 1_048_576
	convertedSSEIdleTimeout = 30 * time.Second
)

type convertedSSELine struct {
	line string
	err  error
}

func scanConvertedSSELines(ctx context.Context, body io.Reader) <-chan convertedSSELine {
	lines := make(chan convertedSSELine, 1)
	go func() {
		defer close(lines)
		scanner := bufio.NewScanner(body)
		scanner.Buffer(make([]byte, 64*1024), convertedSSELimitBytes+1)
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
	return []byte(`{"error":{"code":"upstream_error","message":"The upstream response could not be used.","type":"api_error"}}`)
}

func safeConvertedHeaders(source http.Header) http.Header {
	header := make(http.Header)
	if retryAfter := source.Get("Retry-After"); retryAfter != "" {
		header.Set("Retry-After", retryAfter)
	}
	header.Set("Content-Type", "application/json")
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
) int {
	defer resp.Body.Close()
	peek := make([]byte, 4096)
	n, peekErr := resp.Body.Read(peek)
	if r.Context().Err() != nil {
		return dispatchDone
	}
	class := classifyHTTP(resp.StatusCode)
	if shouldFailover(class, false) {
		pool.ReportFailure(member.ID, model, class, parseRetryAfter(resp.Header.Get("Retry-After")), time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = resp.StatusCode
		*lastHeader = resp.Header.Clone()
		*lastBody = append([]byte(nil), peek[:n]...)
		return dispatchContinue
	}
	if n == 0 && peekErr != nil && peekErr != io.EOF {
		if r.Context().Err() != nil {
			return dispatchDone
		}
		pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
		*excluded = append(*excluded, member.ID)
		return dispatchContinue
	}
	if resp.StatusCode < 400 {
		pool.ReportSuccess(member.ID)
	}
	copyUpstreamHeaders(w.Header(), resp.Header)
	if w.Header().Get("Content-Type") == "" {
		w.Header().Set("Content-Type", "text/event-stream")
	}
	w.WriteHeader(resp.StatusCode)
	if n > 0 {
		if _, err := w.Write(peek[:n]); err != nil {
			return dispatchDone
		}
		if flusher, ok := w.(http.Flusher); ok {
			flusher.Flush()
		}
	}
	if peekErr == io.EOF {
		return dispatchDone
	}
	copySSE(w, resp.Body)
	return dispatchDone
}

func doMemberMessages(ctx context.Context, client *http.Client, member *PoolMember, upstreamPath string, body []byte, stream bool) (*http.Response, error) {
	upstream := joinUpstreamPath(member.UpstreamBaseURL, upstreamPath)
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

func newUpstreamHTTPClient() *http.Client {
	return &http.Client{
		CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
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

func copyUpstreamHeaders(dst, src http.Header) {
	for k, vs := range src {
		if strings.EqualFold(k, "Content-Length") || strings.EqualFold(k, "Authorization") {
			continue
		}
		for _, v := range vs {
			dst.Add(k, v)
		}
	}
}

func writeClientResponse(w http.ResponseWriter, status int, header http.Header, body []byte, stream bool) {
	copyUpstreamHeaders(w.Header(), header)
	if w.Header().Get("Content-Type") == "" {
		if stream {
			w.Header().Set("Content-Type", "text/event-stream")
		} else {
			w.Header().Set("Content-Type", "application/json")
		}
	}
	w.WriteHeader(status)
	if len(body) > 0 {
		_, _ = w.Write(body)
	}
}

func copySSE(w http.ResponseWriter, r io.Reader) {
	flusher, _ := w.(http.Flusher)
	buf := make([]byte, 4096)
	for {
		n, err := r.Read(buf)
		if n > 0 {
			if _, writeErr := w.Write(buf[:n]); writeErr != nil {
				return
			}
			if flusher != nil {
				flusher.Flush()
			}
		}
		if err != nil {
			return
		}
	}
}
