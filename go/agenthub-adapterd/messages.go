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

const (
	edgeStatusRouteBusy           = "route_busy"
	edgeStatusRequestCanceled     = "request_canceled"
	edgeStatusDownstreamWrite     = "downstream_write_failed"
	edgeStatusInvalidRequest      = "invalid_request"
	edgeStatusRequestUnauthorized = "request_unauthorized"
	edgeStatusRequestNotFound     = "request_not_found"
	edgeStatusRequestTooLarge     = "request_too_large"
	edgeStatusUpstreamUnavailable = "upstream_unavailable"
	edgeStatusRequestFailed       = "request_failed"
)

// edgeStatusResponseWriter observes only the downstream status class. It
// does not buffer, retain, or inspect response bodies, so request and login
// material cannot enter the per-edge status counters.
type edgeStatusResponseWriter struct {
	http.ResponseWriter
	status    int
	writeFail bool
}

func newEdgeStatusResponseWriter(w http.ResponseWriter) *edgeStatusResponseWriter {
	return &edgeStatusResponseWriter{ResponseWriter: w}
}

func (w *edgeStatusResponseWriter) WriteHeader(status int) {
	if w.status == 0 {
		w.status = status
	}
	w.ResponseWriter.WriteHeader(status)
}

func (w *edgeStatusResponseWriter) Write(body []byte) (int, error) {
	if w.status == 0 {
		w.status = http.StatusOK
	}
	n, err := w.ResponseWriter.Write(body)
	if err != nil {
		w.writeFail = true
	}
	return n, err
}

func (w *edgeStatusResponseWriter) Flush() {
	if w.status == 0 {
		w.status = http.StatusOK
	}
	if flusher, ok := w.ResponseWriter.(http.Flusher); ok {
		flusher.Flush()
	}
}

func (w *edgeStatusResponseWriter) Unwrap() http.ResponseWriter {
	return w.ResponseWriter
}

func (w *edgeStatusResponseWriter) statusCode() int {
	return w.status
}

func (w *edgeStatusResponseWriter) downstreamWriteFailed() bool {
	return w.writeFail
}

func edgeRequestStatusErrorCode(status int, requestCanceled, downstreamWriteFailed bool) string {
	if requestCanceled {
		return edgeStatusRequestCanceled
	}
	if downstreamWriteFailed {
		return edgeStatusDownstreamWrite
	}
	switch status {
	case http.StatusBadRequest:
		return edgeStatusInvalidRequest
	case http.StatusUnauthorized, http.StatusForbidden:
		return edgeStatusRequestUnauthorized
	case http.StatusNotFound:
		return edgeStatusRequestNotFound
	case http.StatusRequestEntityTooLarge:
		return edgeStatusRequestTooLarge
	case http.StatusServiceUnavailable, http.StatusBadGateway, http.StatusGatewayTimeout:
		return edgeStatusUpstreamUnavailable
	default:
		return edgeStatusRequestFailed
	}
}

func safeEdgeStatusErrorCode(code string) string {
	switch code {
	case edgeStatusRouteBusy:
		return code
	default:
		return edgeStatusRequestFailed
	}
}

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
		edge.rejectRequest(edgeStatusRouteBusy)
		w.Header().Set("Retry-After", "1")
		writeMessagesError(w, http.StatusServiceUnavailable, "route_busy", "The local route is busy. Try again shortly.", "api_error")
		return
	}
	defer rt.releaseRequestSlot()
	observed := newEdgeStatusResponseWriter(w)
	w = observed
	// OAuth refresh can replace edge for an upstream retry. Status remains
	// attributed to the authenticated ingress edge that accepted this request.
	statusEdge := edge
	rt.addInFlight(1)
	statusEdge.beginRequest()
	defer func() {
		rt.addInFlight(-1)
		statusEdge.finishRequest(
			observed.statusCode(),
			r.Context().Err() != nil,
			observed.downstreamWriteFailed(),
		)
	}()

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
		Stream             bool   `json:"stream"`
		Model              string `json:"model"`
		PreviousResponseID string `json:"previous_response_id"`
	}
	_ = json.Unmarshal(body, &meta)
	continuation := strings.TrimSpace(meta.PreviousResponseID) != ""
	affinitySeed := officialAffinitySeed(r.Header, body)

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
	var pinnedMemberID string
	if continuation {
		switch edge.Dialect {
		case "grok", "codex":
			var ok bool
			pinnedMemberID, ok = edge.GrokAffinity.lookupResponse(meta.PreviousResponseID)
			if !ok && affinitySeed != "" {
				pinnedMemberID, ok = edge.GrokAffinity.lookupSeed(affinitySeed)
			}
			if !ok {
				writeMessagesError(w, http.StatusBadRequest, "continuation_unavailable", "This response cannot be continued by the current route.", "invalid_request_error")
				return
			}
		}
	} else if affinitySeed != "" {
		pinnedMemberID, _ = edge.GrokAffinity.lookupSeed(affinitySeed)
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
	var retryGrok *grokOfficialPreparedRequest
	grokDecodeRetried := make(map[string]bool)

	for {
		if r.Context().Err() != nil {
			return
		}
		member := retryMember
		retryMember = nil
		if member == nil {
			if pinnedMemberID != "" {
				member = pool.MemberByIDForModel(pinnedMemberID, model, time.Now())
			} else {
				member = pool.Pick(model, excluded, time.Now())
			}
		}
		if member == nil {
			if pinnedMemberID != "" {
				writeMessagesError(w, http.StatusBadRequest, "continuation_unavailable", "The original login is not available for this continuation.", "invalid_request_error")
				return
			}
			if hasLast {
				writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
				return
			}
			writeMessagesError(w, http.StatusServiceUnavailable, "pool_exhausted", "No eligible pool member remains.", "api_error")
			return
		}

		memberPath := upstreamPath
		memberBody := body
		downstreamStream := meta.Stream
		upstreamStream := meta.Stream
		officialCodex := member.UpstreamTarget == upstreamTargetCodexChatGPTSubscription
		officialGrok := member.UpstreamTarget == upstreamTargetGrokXAISubscription
		pairDirection := edge.officialPairDirection(member)
		var grokPrepared *grokOfficialPreparedRequest
		convertChatResponse := surface == surfaceResponses && member.UpstreamTransport == transportOpenAIChatCompletions
		if convertChatResponse {
			memberPath = "/v1/chat/completions"
			memberBody, upstreamStream, err = encodeResponsesToChat(body)
			if err != nil {
				writeMessagesError(w, http.StatusBadRequest, "invalid_request", "The Responses request cannot be represented by this route.", "invalid_request_error")
				return
			}
		} else if officialCodex {
			if pairDirection == officialPairGrokToCodex {
				memberBody, downstreamStream, err = prepareGrokIngressCodexRequest(body, model)
			} else {
				memberBody, downstreamStream, err = prepareOfficialCodexRequest(body, model)
			}
			if err != nil {
				writeMessagesError(w, http.StatusBadRequest, "invalid_request", "The Responses request cannot be represented by this route.", "invalid_request_error")
				return
			}
			upstreamStream = true
		} else if officialGrok {
			if retryGrok != nil {
				grokPrepared = retryGrok
				retryGrok = nil
			} else {
				grokInput := body
				if pairDirection == officialPairCodexToGrok {
					grokInput, downstreamStream, err = prepareCodexIngressGrokRequest(body)
				}
				if err == nil {
					grokPrepared, err = prepareGrokOfficialRequest(grokInput, r.Header, member.SourceID, "", model)
				}
				if err == nil {
					edge.GrokReplay.apply(grokPrepared.Body, grokPrepared.SourceID, grokPrepared.Model, grokPrepared.CacheSeed)
				}
			}
			if err == nil {
				memberBody, err = grokPrepared.marshalBody()
			}
			if err != nil {
				writeMessagesError(w, http.StatusBadRequest, "invalid_request", "The Responses request cannot be represented by this route.", "invalid_request_error")
				return
			}
		}

		resp, err := doMemberMessagesWithIdentity(r.Context(), client, member, memberPath, memberBody, upstreamStream, grokPrepared)
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
			if continuation {
				writeClientResponse(w, lastStatus, lastHeader, lastBody, false)
				return
			}
			continue
		}
		if resp.StatusCode == http.StatusUnauthorized && !refreshUsed && member.RefreshKind != refreshNone {
			refreshUsed = true
			_ = resp.Body.Close()
			if refreshed := rt.requestOAuthRefresh(r.Context(), edge.ID, member); refreshed != nil && refreshed.member.serves(model, time.Now()) {
				if refreshed.edge == nil {
					writeSafeUpstreamResponse(w, http.StatusBadGateway, make(http.Header))
					return
				}
				edge = refreshed.edge
				pool = refreshed.pool
				retryMember = refreshed.member
				continue
			}
		}
		if resp.StatusCode >= 300 {
			errorBody, readErr := readBoundedResponseBody(
				r.Context(), resp, rt.httpPolicy.NonStreamBodyBytes,
				rt.httpPolicy.NonStreamIdleTimeout, rt.httpPolicy.NonStreamTotalTimeout,
			)
			_ = resp.Body.Close()
			if readErr != nil {
				errorBody = nil
			}
			if officialGrok && resp.StatusCode == http.StatusBadRequest && grokPrepared != nil &&
				edge.GrokReplay.recoverDecodeFailure(grokPrepared.Body, grokPrepared.SourceID, grokPrepared.Model, grokPrepared.CacheSeed, errorBody, grokDecodeRetried[member.ID]) {
				grokDecodeRetried[member.ID] = true
				retryGrok = grokPrepared
				retryMember = member
				continue
			}
			class := classifyHTTPBody(resp.StatusCode, errorBody)
			if continuation {
				class = classRequest
			}
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

		if downstreamStream {
			if convertChatResponse {
				outcome := dispatchConvertedChatStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast, rt.httpPolicy)
				lastStream = false
				if outcome == dispatchContinue {
					if continuation {
						writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
						return
					}
					continue
				}
				return
			}
			if officialCodex {
				var sanitizer officialSSEEventSanitizer
				if pairDirection == officialPairGrokToCodex {
					sanitizer = sanitizeCodexOfficialSSEEventForGrok
				}
				var streamAffinity officialStreamAffinity
				outcome := dispatchOfficialResponsesStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast, rt.httpPolicy, sanitizer, &streamAffinity)
				if outcome == dispatchDone && streamAffinity.Completed {
					edge.GrokAffinity.storeResponse(streamAffinity.ResponseID, member.ID)
					edge.GrokAffinity.storeSeed(affinitySeed, member.ID)
				}
				if outcome == dispatchContinue {
					lastStream = false
					if continuation {
						writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
						return
					}
					continue
				}
				lastStream = true
				return
			}
			if officialGrok {
				captured := newCappedCapture(grokOfficialReplayBodyBytes)
				resp.Body = struct {
					io.Reader
					io.Closer
				}{Reader: io.TeeReader(resp.Body, captured), Closer: resp.Body}
				var sanitizer officialSSEEventSanitizer
				if pairDirection == officialPairCodexToGrok {
					sanitizer = sanitizeGrokOfficialSSEEventForCodex
				}
				var streamAffinity officialStreamAffinity
				outcome := dispatchGrokResponsesStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast, rt.httpPolicy, sanitizer, &streamAffinity)
				if outcome == dispatchDone && streamAffinity.Completed {
					edge.GrokAffinity.storeResponse(streamAffinity.ResponseID, member.ID)
					edge.GrokAffinity.storeSeed(affinitySeed, member.ID)
					if !captured.Overflowed() {
						edge.GrokReplay.storeSSE(grokPrepared.SourceID, grokPrepared.Model, grokPrepared.CacheSeed, captured.Bytes())
					}
				}
				if outcome == dispatchContinue {
					lastStream = false
					if continuation {
						writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
						return
					}
					continue
				}
				lastStream = true
				return
			}
			outcome := dispatchMemberStream(w, r, pool, member, model, resp, &excluded, &lastStatus, &lastHeader, &lastBody, &hasLast, rt.httpPolicy)
			lastStream = true
			if outcome == dispatchContinue {
				if continuation {
					writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
					return
				}
				continue
			}
			return
		}

		responseLimit := rt.httpPolicy.NonStreamBodyBytes
		responseIdle := rt.httpPolicy.NonStreamIdleTimeout
		responseTotal := rt.httpPolicy.NonStreamTotalTimeout
		if officialCodex {
			responseLimit = rt.httpPolicy.SSEBodyBytes
			responseIdle = rt.httpPolicy.SSEIdleTimeout
		}
		respBody, readErr := readBoundedResponseBody(
			r.Context(),
			resp,
			responseLimit,
			responseIdle,
			responseTotal,
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
			if continuation {
				writeClientResponse(w, lastStatus, lastHeader, lastBody, false)
				return
			}
			continue
		}
		if officialCodex {
			if !isEventStreamMediaType(resp.Header.Get("Content-Type")) {
				readErr = errInvalidResponsesSSE
			} else {
				respBody, readErr = aggregateOfficialResponsesSSE(respBody, rt.httpPolicy.SSEBodyBytes)
			}
			if readErr == nil {
				resp.Header.Set("Content-Type", "application/json")
			}
		}
		if errors.Is(readErr, errOfficialResponsesFailure) {
			pool.ReportFailure(member.ID, model, classRequest, 0, time.Now())
			writeSafeUpstreamResponse(w, http.StatusBadGateway, make(http.Header))
			return
		}
		if readErr == nil && pairDirection == officialPairGrokToCodex {
			respBody, readErr = sanitizeCodexOfficialResponseForGrok(respBody)
		} else if readErr == nil && pairDirection == officialPairCodexToGrok {
			respBody, readErr = sanitizeGrokOfficialResponseForCodex(respBody)
		}
		if readErr != nil || (!officialCodex && (!isJSONMediaType(resp.Header.Get("Content-Type")) || !isJSONObject(respBody))) {
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			excluded = append(excluded, member.ID)
			hasLast = true
			lastStatus = http.StatusBadGateway
			lastHeader = make(http.Header)
			lastBody = safeUpstreamErrorBody()
			lastStream = false
			if continuation {
				writeClientResponse(w, lastStatus, lastHeader, lastBody, false)
				return
			}
			continue
		}
		var grokCompleted map[string]any
		if officialGrok {
			if json.Unmarshal(respBody, &grokCompleted) != nil {
				writeSafeUpstreamResponse(w, http.StatusBadGateway, make(http.Header))
				return
			}
			if grokOfficialString(grokCompleted["status"]) == "failed" || grokCompleted["error"] != nil || grokOfficialString(grokCompleted["type"]) == "error" {
				pool.ReportFailure(member.ID, model, classRequest, 0, time.Now())
				writeSafeUpstreamResponse(w, http.StatusBadGateway, make(http.Header))
				return
			}
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
				if continuation {
					writeClientResponse(w, lastStatus, lastHeader, lastBody, false)
					return
				}
				continue
			}
			respBody = translated
			resp.Header.Set("Content-Type", "application/json")
		}
		if officialGrok && grokOfficialString(grokCompleted["status"]) == "completed" {
			if grokPrepared != nil && len(respBody) <= grokOfficialReplayBodyBytes {
				edge.GrokReplay.storeCompleted(grokPrepared.SourceID, grokPrepared.Model, grokPrepared.CacheSeed, grokCompleted)
			}
			responseID := grokOfficialResponseID(grokCompleted)
			edge.GrokAffinity.storeResponse(responseID, member.ID)
			edge.GrokAffinity.storeSeed(affinitySeed, member.ID)
		} else if officialCodex {
			var completed map[string]any
			if json.Unmarshal(respBody, &completed) == nil {
				responseID := grokOfficialResponseID(completed)
				edge.GrokAffinity.storeResponse(responseID, member.ID)
				edge.GrokAffinity.storeSeed(affinitySeed, member.ID)
			}
		}
		pool.ReportSuccess(member.ID)
		writeClientResponse(w, resp.StatusCode, resp.Header, respBody, false)
		return
	}
}

func officialAffinitySeed(headers http.Header, raw []byte) string {
	var body map[string]any
	if json.Unmarshal(raw, &body) != nil || body == nil {
		return ""
	}
	return extractGrokOfficialPromptCacheSeed(headers, body)
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
	return doMemberMessagesWithIdentity(ctx, client, member, upstreamPath, body, stream, nil)
}

func doMemberMessagesWithIdentity(ctx context.Context, client *http.Client, member *PoolMember, upstreamPath string, body []byte, stream bool, grok *grokOfficialPreparedRequest) (*http.Response, error) {
	upstream, err := buildFinalUpstreamURL(
		member.UpstreamBaseURL,
		upstreamPath,
		member.UpstreamTransport,
		member.UpstreamAuth,
		member.UpstreamTarget,
		member.CredentialClass,
	)
	if err != nil {
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
	} else if member.UpstreamTarget == upstreamTargetCodexChatGPTSubscription {
		if member.UpstreamKey == "" || !validOfficialAccountID(member.OfficialAccountID) {
			return nil, errors.New("official Codex request identity is invalid")
		}
		req.Header.Set("Authorization", "Bearer "+member.UpstreamKey)
		req.Header.Set("ChatGPT-Account-ID", member.OfficialAccountID)
		req.Header.Set("Accept", "text/event-stream")
		req.Header.Set("OpenAI-Beta", "responses=experimental")
		req.Header.Set("Originator", "codex-tui")
		req.Header.Set("Version", "0.146.0")
		req.Header.Set("User-Agent", "codex-tui/0.146.0")
	} else if member.UpstreamTarget == upstreamTargetGrokXAISubscription {
		if grok == nil || applyGrokOfficialHeaders(req, member.UpstreamKey, grok.Identity) != nil {
			return nil, errors.New("official Grok request identity is invalid")
		}
	} else if member.UpstreamKey != "" {
		req.Header.Set("Authorization", "Bearer "+member.UpstreamKey)
	}
	return client.Do(req)
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
