package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

const fixtureAssistantText = "isolated-messages-ok"

const (
	memberHi              = "sk-member-hi-synthetic"
	memberLo              = "sk-member-lo-synthetic"
	memberA               = "sk-member-a-synthetic"
	memberB               = "sk-member-b-synthetic"
	memberQuota           = "sk-member-quota-synthetic"
	memberDown            = "sk-member-down-synthetic"
	memberSlow            = "sk-member-slow-synthetic"
	memberDrain           = "sk-member-drain-synthetic"
	memberCommit          = "sk-member-commit-synthetic"
	memberHeaderMessages  = "sk-member-header-messages-synthetic"
	memberHeaderResponses = "sk-member-header-responses-synthetic"
	memberHeaderChat      = "sk-member-header-chat-synthetic"
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
	mux.HandleFunc("/v1/responses", mockResponses)
	mux.HandleFunc("/v1/chat/completions", mockChat)
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
	token := mockUpstreamToken(r)
	if !mockHeaderContractValid(r, token) {
		http.Error(w, "invalid upstream auth headers", http.StatusBadRequest)
		return
	}

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
	case memberDrain:
		writeDrainSuccess(w, r, stream, model)
	case memberCommit:
		writeCommitThenClose(w, model)
	case memberHeaderMessages:
		writeFixtureSuccess(w, stream, model, "isolated-header-messages-ok")
	default:
		writeUnknownMember(w)
	}
}

func mockResponses(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	body, _ := io.ReadAll(io.LimitReader(r.Body, 8<<20))
	stream, model, tools := parseResponsesReq(body)
	token := mockUpstreamToken(r)
	if !mockHeaderContractValid(r, token) {
		http.Error(w, "invalid upstream auth headers", http.StatusBadRequest)
		return
	}

	switch token {
	case memberQuota:
		writeQuotaError(w)
	case memberDown:
		writeDownError(w)
	case memberSlow:
		writeSlowResponses(w, r, stream, model, tools)
	case memberCommit:
		writeResponsesCommit(w, model)
	case memberHi:
		writeResponsesMember(w, stream, model, tools, "isolated-responses-ok")
	case memberLo:
		writeResponsesMember(w, stream, model, tools, "isolated-responses-lo")
	case memberA:
		writeResponsesMember(w, stream, model, tools, "isolated-responses-a")
	case memberB:
		writeResponsesMember(w, stream, model, tools, "isolated-responses-b")
	case memberHeaderResponses:
		writeResponsesMember(w, stream, model, tools, "isolated-header-responses-ok")
	default:
		// Missing Authorization stays a Messages-only success. Responses requires a known member bearer.
		writeUnknownMember(w)
	}
}

func parseResponsesReq(body []byte) (stream bool, model string, tools bool) {
	var req struct {
		Stream bool   `json:"stream"`
		Model  string `json:"model"`
	}
	_ = json.Unmarshal(body, &req)
	model = req.Model
	if model == "" {
		model = "gpt-5-probe"
	}
	return req.Stream, model, bytes.Contains(body, []byte(`"tools"`))
}

func writeResponsesMember(w http.ResponseWriter, stream bool, model string, tools bool, text string) {
	if stream && tools {
		writeResponsesToolSSE(w, model)
		return
	}
	if stream {
		writeResponsesTextSSE(w, model, text)
		return
	}
	writeResponsesJSON(w, model, text)
}

func writeResponsesJSON(w http.ResponseWriter, model, text string) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"id":     "resp_probe_fixture",
		"object": "response",
		"status": "completed",
		"model":  model,
		"output": []any{
			map[string]any{
				"id":     "msg_probe_fixture",
				"type":   "message",
				"role":   "assistant",
				"status": "completed",
				"content": []any{
					map[string]any{"type": "output_text", "text": text},
				},
			},
		},
	})
}

func writeResponsesTextSSE(w http.ResponseWriter, model, text string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	writeSSEFrame(w, flusher, "response.created", map[string]any{
		"type": "response.created",
		"response": map[string]any{
			"id":     "resp_probe_text",
			"object": "response",
			"status": "in_progress",
			"model":  model,
			"output": []any{},
		},
	})
	writeSSEFrame(w, flusher, "response.output_text.delta", map[string]any{
		"type":  "response.output_text.delta",
		"delta": text,
	})
	writeSSEFrame(w, flusher, "response.completed", map[string]any{
		"type": "response.completed",
		"response": map[string]any{
			"id":     "resp_probe_text",
			"object": "response",
			"status": "completed",
			"model":  model,
			"output": []any{
				map[string]any{
					"type": "message",
					"role": "assistant",
					"content": []any{
						map[string]any{"type": "output_text", "text": text},
					},
				},
			},
		},
	})
}

func writeResponsesToolSSE(w http.ResponseWriter, model string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	const (
		respID = "resp_probe_tool"
		itemID = "fc_probe_weather"
		callID = "call_probe_weather"
		args   = `{"city":"Paris"}`
	)
	itemInProgress := map[string]any{
		"id":        itemID,
		"type":      "function_call",
		"status":    "in_progress",
		"name":      "weather",
		"call_id":   callID,
		"arguments": "",
	}
	itemDone := map[string]any{
		"id":        itemID,
		"type":      "function_call",
		"status":    "completed",
		"name":      "weather",
		"call_id":   callID,
		"arguments": args,
	}
	writeSSEFrame(w, flusher, "response.created", map[string]any{
		"type": "response.created",
		"response": map[string]any{
			"id":     respID,
			"object": "response",
			"status": "in_progress",
			"model":  model,
			"output": []any{},
		},
	})
	writeSSEFrame(w, flusher, "response.output_item.added", map[string]any{
		"type":         "response.output_item.added",
		"output_index": 0,
		"item":         itemInProgress,
	})
	writeSSEFrame(w, flusher, "response.function_call_arguments.delta", map[string]any{
		"type":         "response.function_call_arguments.delta",
		"output_index": 0,
		"item_id":      itemID,
		"delta":        args,
	})
	writeSSEFrame(w, flusher, "response.function_call_arguments.done", map[string]any{
		"type":         "response.function_call_arguments.done",
		"output_index": 0,
		"item_id":      itemID,
		"arguments":    args,
	})
	writeSSEFrame(w, flusher, "response.output_item.done", map[string]any{
		"type":         "response.output_item.done",
		"output_index": 0,
		"item":         itemDone,
	})
	writeSSEFrame(w, flusher, "response.completed", map[string]any{
		"type": "response.completed",
		"response": map[string]any{
			"id":     respID,
			"object": "response",
			"status": "completed",
			"model":  model,
			"output": []any{itemDone},
		},
	})
}

func writeResponsesCommit(w http.ResponseWriter, model string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	writeSSEFrame(w, flusher, "response.created", map[string]any{
		"type": "response.created",
		"response": map[string]any{
			"id":     "resp_probe_commit",
			"object": "response",
			"status": "in_progress",
			"model":  model,
			"output": []any{},
		},
	})
}

func writeSSEFrame(w http.ResponseWriter, flusher http.Flusher, event string, data any) {
	payload, err := json.Marshal(data)
	if err != nil {
		return
	}
	_, _ = fmt.Fprintf(w, "event: %s\ndata: %s\n\n", event, payload)
	if flusher != nil {
		flusher.Flush()
	}
}

func mockChat(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	body, _ := io.ReadAll(io.LimitReader(r.Body, 8<<20))
	stream, model, tools := parseResponsesReq(body)
	token := mockUpstreamToken(r)
	if !mockHeaderContractValid(r, token) {
		http.Error(w, "invalid upstream auth headers", http.StatusBadRequest)
		return
	}
	switch token {
	case memberQuota:
		writeQuotaError(w)
	case memberDown:
		writeDownError(w)
	case memberSlow:
		writeSlowChat(w, r, stream, model, tools)
	case memberCommit:
		writeChatCommit(w, model)
	case memberHi:
		writeChatMember(w, stream, model, tools, "isolated-chat-ok")
	case memberLo:
		writeChatMember(w, stream, model, tools, "isolated-chat-lo")
	case memberA:
		writeChatMember(w, stream, model, tools, "isolated-chat-a")
	case memberB:
		writeChatMember(w, stream, model, tools, "isolated-chat-b")
	case memberHeaderChat:
		writeChatMember(w, stream, model, tools, "isolated-header-chat-ok")
	default:
		writeUnknownMember(w)
	}
}

func mockUpstreamToken(r *http.Request) string {
	if token := bearerToken(r.Header.Get("Authorization")); token != "" {
		return token
	}
	return strings.TrimSpace(r.Header.Get("X-API-Key"))
}

func mockHeaderContractValid(r *http.Request, token string) bool {
	authorization := strings.TrimSpace(r.Header.Get("Authorization"))
	apiKey := strings.TrimSpace(r.Header.Get("X-API-Key"))
	switch token {
	case memberHeaderMessages:
		return authorization == "" && apiKey == token && r.Header.Get("Anthropic-Version") == "2023-06-01"
	case memberHeaderResponses, memberHeaderChat:
		return bearerToken(authorization) == token && apiKey == ""
	default:
		return true
	}
}

func writeChatMember(w http.ResponseWriter, stream bool, model string, tools bool, text string) {
	if stream && tools {
		writeChatToolSSE(w, model)
		return
	}
	if stream {
		writeChatTextSSE(w, model, text)
		return
	}
	writeChatJSON(w, model, text)
}

func writeChatJSON(w http.ResponseWriter, model, text string) {
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(map[string]any{
		"id":      "chatcmpl_probe_fixture",
		"object":  "chat.completion",
		"created": 1720000000,
		"model":   model,
		"choices": []any{
			map[string]any{
				"index": 0,
				"message": map[string]any{
					"role":    "assistant",
					"content": text,
				},
				"finish_reason": "stop",
			},
		},
	})
}

func writeChatTextSSE(w http.ResponseWriter, model, text string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	writeChatChunk(w, flusher, model, map[string]any{
		"role":    "assistant",
		"content": text,
	}, nil)
	writeChatDone(w, flusher)
}

func writeChatToolSSE(w http.ResponseWriter, model string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	writeChatChunk(w, flusher, model, map[string]any{
		"role": "assistant",
		"tool_calls": []any{
			map[string]any{
				"index": 0,
				"id":    "call_probe_weather",
				"type":  "function",
				"function": map[string]any{
					"name":      "weather",
					"arguments": `{"city":`,
				},
			},
		},
	}, nil)
	writeChatChunk(w, flusher, model, map[string]any{
		"tool_calls": []any{
			map[string]any{
				"index": 0,
				"function": map[string]any{
					"arguments": `"Paris"}`,
				},
			},
		},
	}, "tool_calls")
	writeChatDone(w, flusher)
}

func writeChatCommit(w http.ResponseWriter, model string) {
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.WriteHeader(http.StatusOK)
	flusher, _ := w.(http.Flusher)
	writeChatChunk(w, flusher, model, map[string]any{
		"role":    "assistant",
		"content": "",
	}, nil)
}

func writeChatChunk(w http.ResponseWriter, flusher http.Flusher, model string, delta map[string]any, finish any) {
	payload, err := json.Marshal(map[string]any{
		"id":      "chatcmpl_probe_stream",
		"object":  "chat.completion.chunk",
		"created": 1720000000,
		"model":   model,
		"choices": []any{
			map[string]any{
				"index":         0,
				"delta":         delta,
				"finish_reason": finish,
			},
		},
	})
	if err != nil {
		return
	}
	_, _ = fmt.Fprintf(w, "data: %s\n\n", payload)
	if flusher != nil {
		flusher.Flush()
	}
}

func writeChatDone(w http.ResponseWriter, flusher http.Flusher) {
	_, _ = io.WriteString(w, "data: [DONE]\n\n")
	if flusher != nil {
		flusher.Flush()
	}
}

func writeSlowChat(w http.ResponseWriter, r *http.Request, stream bool, model string, tools bool) {
	timer := time.NewTimer(8 * time.Second)
	defer timer.Stop()
	select {
	case <-r.Context().Done():
		return
	case <-timer.C:
		writeChatMember(w, stream, model, tools, "isolated-chat-slow")
	}
}

func writeSlowResponses(w http.ResponseWriter, r *http.Request, stream bool, model string, tools bool) {
	timer := time.NewTimer(8 * time.Second)
	defer timer.Stop()
	select {
	case <-r.Context().Done():
		return
	case <-timer.C:
		writeResponsesMember(w, stream, model, tools, "isolated-member-slow")
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

func writeDrainSuccess(w http.ResponseWriter, r *http.Request, stream bool, model string) {
	timer := time.NewTimer(2 * time.Second)
	defer timer.Stop()
	select {
	case <-r.Context().Done():
		return
	case <-timer.C:
		writeFixtureSuccess(w, stream, model, "isolated-messages-drained")
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
