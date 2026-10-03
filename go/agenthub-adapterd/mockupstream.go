package main

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"time"
)

const fixtureAssistantText = "isolated-messages-ok"

const (
	memberHi     = "sk-member-hi-synthetic"
	memberLo     = "sk-member-lo-synthetic"
	memberA      = "sk-member-a-synthetic"
	memberB      = "sk-member-b-synthetic"
	memberQuota  = "sk-member-quota-synthetic"
	memberDown   = "sk-member-down-synthetic"
	memberSlow   = "sk-member-slow-synthetic"
	memberCommit = "sk-member-commit-synthetic"
)

func runMockUpstream(listen string) error {
	if listen == "" {
		listen = "127.0.0.1:0"
	}
	ln, err := net.Listen("tcp", listen)
	if err != nil {
		return err
	}
	addr, ok := ln.Addr().(*net.TCPAddr)
	if !ok || addr.IP == nil || !addr.IP.IsLoopback() {
		_ = ln.Close()
		return fmt.Errorf("mock upstream must bind loopback")
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/v1/messages", mockMessages)
	srv := &http.Server{
		Handler:           mux,
		ReadHeaderTimeout: 10 * time.Second,
	}
	fmt.Fprintf(os.Stdout, "agenthub-adapterd mock-upstream listening on %s\n", addr.String())
	log.Printf("mock-upstream listening on %s", addr.String())
	return srv.Serve(ln)
}

func mockMessages(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	body, _ := io.ReadAll(io.LimitReader(r.Body, 8<<20))
	stream, model := parseMockReq(body)
	token := bearerToken(r.Header.Get("Authorization"))

	switch token {
	case "":
		// Default fixture used by messages-isolated.sh (no Authorization).
		writeFixtureSuccess(w, stream, model, fixtureAssistantText)
	case memberHi:
		writeFixtureSuccess(w, stream, model, "isolated-member-hi")
	case memberLo:
		writeFixtureSuccess(w, stream, model, "isolated-member-lo")
	case memberA:
		writeFixtureSuccess(w, stream, model, "isolated-member-a")
	case memberB:
		writeFixtureSuccess(w, stream, model, "isolated-member-b")
	case memberQuota:
		writeQuotaError(w)
	case memberDown:
		writeDownError(w)
	case memberSlow:
		writeSlowSuccess(w, r, stream, model)
	case memberCommit:
		writeCommitThenClose(w, model)
	default:
		writeUnknownMember(w)
	}
}

func parseMockReq(body []byte) (stream bool, model string) {
	var req struct {
		Stream bool   `json:"stream"`
		Model  string `json:"model"`
	}
	_ = json.Unmarshal(body, &req)
	model = req.Model
	if model == "" {
		model = "claude-probe-fixture"
	}
	return req.Stream, model
}

func writeFixtureSuccess(w http.ResponseWriter, stream bool, model, text string) {
	if stream {
		writeFixtureSSE(w, model, text)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"id":   "msg_probe_fixture",
		"type": "message",
		"role": "assistant",
		"content": []map[string]string{
			{"type": "text", "text": text},
		},
		"model":       model,
		"stop_reason": "end_turn",
		"usage":       map[string]int{"input_tokens": 1, "output_tokens": 4},
	})
}

func writeFixtureSSE(w http.ResponseWriter, model, text string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	modelJSON, _ := json.Marshal(model)
	textJSON, _ := json.Marshal(text)
	frames := []string{
		"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_probe_fixture\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":" + string(modelJSON) + "}}\n\n",
		"event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
		"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":" + string(textJSON) + "}}\n\n",
		"event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
		"event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
		"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
	}
	for _, frame := range frames {
		_, _ = io.WriteString(w, frame)
		if flusher != nil {
			flusher.Flush()
		}
	}
}

func writeJSONError(w http.ResponseWriter, status int, code, message, typ string, extra http.Header) {
	for k, vs := range extra {
		for _, v := range vs {
			w.Header().Add(k, v)
		}
	}
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

func writeQuotaError(w http.ResponseWriter) {
	h := make(http.Header)
	h.Set("Retry-After", "1")
	writeJSONError(w, http.StatusTooManyRequests, "rate_limited", "quota exhausted", "rate_limit_error", h)
}

func writeDownError(w http.ResponseWriter) {
	writeJSONError(w, http.StatusServiceUnavailable, "unavailable", "upstream unavailable", "api_error", nil)
}

func writeUnknownMember(w http.ResponseWriter) {
	writeJSONError(w, http.StatusUnauthorized, "invalid_api_key", "unknown fixture member", "authentication_error", nil)
}

func writeSlowSuccess(w http.ResponseWriter, r *http.Request, stream bool, model string) {
	timer := time.NewTimer(8 * time.Second)
	defer timer.Stop()
	select {
	case <-r.Context().Done():
		return
	case <-timer.C:
		writeFixtureSuccess(w, stream, model, "isolated-member-slow")
	}
}

func writeCommitThenClose(w http.ResponseWriter, model string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	modelJSON, _ := json.Marshal(model)
	frame := "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_probe_commit\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":" + string(modelJSON) + "}}\n\n"
	_, _ = io.WriteString(w, frame)
	if flusher != nil {
		flusher.Flush()
	}
}
