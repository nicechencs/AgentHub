package main

import (
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
	mux.HandleFunc("/v1/models", rt.handleModels)
	mux.HandleFunc("/models", rt.handleModels)
	mux.HandleFunc("/health", rt.handleHealth)
	return mux
}

func (rt *Runtime) handleMessages(w http.ResponseWriter, r *http.Request) {
	if r.RemoteAddr != "" && !isLoopbackRemote(r.RemoteAddr) {
		http.Error(w, "loopback only", http.StatusForbidden)
		return
	}
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		writeMessagesError(w, http.StatusMethodNotAllowed, "method_not_allowed", "This endpoint only accepts POST /v1/messages. 本机该路径只接受 POST /v1/messages.", "invalid_request_error")
		return
	}
	if !rt.ownerServing() {
		writeMessagesError(w, http.StatusServiceUnavailable, "bridge_stopping", "Messages listener is not serving.", "invalid_request_error")
		return
	}
	got := bearerToken(r.Header.Get("Authorization"))
	want := rt.ingressKey()
	if want == "" || got != want {
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

	pool := rt.currentPool()
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

	client := &http.Client{}
	excluded := make([]string, 0, 4)
	var lastStatus int
	var lastHeader http.Header
	var lastBody []byte
	var lastStream bool
	hasLast := false

	for {
		if r.Context().Err() != nil {
			return
		}
		member := pool.Pick(model, excluded, time.Now())
		if member == nil {
			if hasLast {
				writeClientResponse(w, lastStatus, lastHeader, lastBody, lastStream)
				return
			}
			writeMessagesError(w, http.StatusServiceUnavailable, "pool_exhausted", "No eligible pool member remains.", "api_error")
			return
		}

		resp, err := doMemberMessages(r.Context(), client, member, body, meta.Stream)
		if err != nil {
			if r.Context().Err() != nil {
				return
			}
			pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
			excluded = append(excluded, member.ID)
			continue
		}

		if meta.Stream {
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
			lastBody = respBody
			lastStream = false
			continue
		}
		if resp.StatusCode < 400 {
			pool.ReportSuccess(member.ID)
		}
		writeClientResponse(w, resp.StatusCode, resp.Header, respBody, false)
		return
	}
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

func doMemberMessages(ctx context.Context, client *http.Client, member *PoolMember, body []byte, stream bool) (*http.Response, error) {
	upstream := member.UpstreamBaseURL + "/v1/messages"
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, upstream, bytes.NewReader(body))
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")
	if stream {
		req.Header.Set("Accept", "text/event-stream")
	}
	if member.UpstreamKey != "" {
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
	want := rt.ingressKey()
	if want == "" || got != want {
		writeMessagesError(w, http.StatusUnauthorized, "invalid_api_key", "Invalid local bearer token.", "invalid_request_error")
		return
	}
	ids := []string{}
	if pool := rt.currentPool(); pool != nil {
		ids = pool.ModelIDs()
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
	rt.mu.Lock()
	listenReady := rt.listenReady
	pool := rt.pool
	rt.mu.Unlock()
	memberCount := 0
	healthyCount := 0
	if pool != nil {
		snap := pool.Snapshot(time.Now())
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
