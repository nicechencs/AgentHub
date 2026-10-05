package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

const (
	tokenCaptureIngress = "ahb_usage_token_capture_ingress_secret"
	tokenCaptureKey     = "sk_usage_token_capture_upstream_secret"
	tokenCaptureBody    = "usage-token-capture-request-body"
	tokenCaptureModel   = "usage-token-capture-model"
)

const tokenCaptureMessagesSSE = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":12,\"cache_read_input_tokens\":4}}}\n\n" +
	"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":6,\"reasoning_tokens\":2}}\n\n" +
	"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"

// tokenCaptureUsageThenFailReadCloser first makes a complete, valid usage
// sequence observable and then fails the upstream stream before completion.
// This proves the final event cannot retain counters from a failed request.
type tokenCaptureUsageThenFailReadCloser struct {
	payload []byte
	sent    bool
}

func (reader *tokenCaptureUsageThenFailReadCloser) Read(buffer []byte) (int, error) {
	if !reader.sent {
		reader.sent = true
		return copy(buffer, reader.payload), nil
	}
	return 0, io.ErrUnexpectedEOF
}

func (*tokenCaptureUsageThenFailReadCloser) Close() error { return nil }

// tokenCaptureUsageThenBlockReadCloser lets the test cancel only after the
// valid usage sequence has been forwarded and observed.
type tokenCaptureUsageThenBlockReadCloser struct {
	payload []byte
	sent    bool
	waiting chan struct{}
	closed  chan struct{}
}

func (reader *tokenCaptureUsageThenBlockReadCloser) Read(buffer []byte) (int, error) {
	if !reader.sent {
		reader.sent = true
		return copy(buffer, reader.payload), nil
	}
	select {
	case reader.waiting <- struct{}{}:
	default:
	}
	<-reader.closed
	return 0, io.ErrClosedPipe
}

func (reader *tokenCaptureUsageThenBlockReadCloser) Close() error {
	select {
	case <-reader.closed:
	default:
		close(reader.closed)
	}
	return nil
}

func tokenCaptureRuntime(t *testing.T, surface, transport, upstream, spool string) *Runtime {
	t.Helper()
	root, err := os.MkdirTemp("", "ahu-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(root) })
	home := filepath.Join(root, "runtime", "adapterd")
	if err := os.MkdirAll(home, 0o700); err != nil {
		t.Fatal(err)
	}
	runtime, err := NewRuntimeWithScope(home, productDefaultPort, "", runtimeScopeProduct, func() {})
	if err != nil {
		t.Fatal(err)
	}
	dialect := "codex"
	auth := authBearer
	if surface == surfaceMessages {
		dialect = "claude"
		auth = authAPIKey
	} else if surface == surfaceChatCompletions {
		dialect = "generic"
	}
	config := &RuntimeConfig{
		Version:       runtimeConfigVersion,
		UsageSpoolDir: spool,
		Edges: []RuntimeEdgeConfig{{
			ID:             "token-capture-pool",
			IngressKey:     tokenCaptureIngress,
			Surface:        surface,
			Dialect:        dialect,
			SchedulePolicy: policyPriorityFailover,
			FixtureModel:   tokenCaptureModel,
			Members: []RuntimeMemberConfig{{
				ID: "token-capture-member", TicketID: "token-capture-ticket", SourceKind: "provider", SourceID: "token-capture-source",
				UpstreamBaseURL: upstream, UpstreamKey: tokenCaptureKey, UpstreamAuth: auth,
				UpstreamTransport: transport, UpstreamModel: tokenCaptureModel,
				Priority: 0, Position: 0, Models: []string{tokenCaptureModel},
			}},
		}},
	}
	if err := runtime.SetRuntimeConfig(config); err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	runtime.listenReady = true
	runtime.lifecycle = lifecycleServing
	runtime.ownerTerm = 1
	runtime.ownerLeaseUntil = time.Now().Add(time.Minute)
	runtime.mu.Unlock()
	return runtime
}

func tokenCaptureRequest(ctx context.Context, surface string, stream bool) *http.Request {
	path := "/v1/messages"
	body := fmt.Sprintf(`{"model":%q,"stream":%t,"messages":[{"role":"user","content":%q}]}`, tokenCaptureModel, stream, tokenCaptureBody)
	switch surface {
	case surfaceResponses:
		path = "/v1/responses"
		body = fmt.Sprintf(`{"model":%q,"stream":%t,"input":%q}`, tokenCaptureModel, stream, tokenCaptureBody)
	case surfaceChatCompletions:
		path = "/v1/chat/completions"
	}
	request := httptest.NewRequest(http.MethodPost, "http://127.0.0.1"+path, strings.NewReader(body)).WithContext(ctx)
	request.RemoteAddr = "127.0.0.1:12345"
	request.Header.Set("Authorization", "Bearer "+tokenCaptureIngress)
	request.Header.Set("Content-Type", "application/json")
	return request
}

func serveTokenCaptureRequest(runtime *Runtime, surface string, request *http.Request) *httptest.ResponseRecorder {
	recorder := httptest.NewRecorder()
	path := "/v1/messages"
	if surface == surfaceResponses {
		path = "/v1/responses"
	} else if surface == surfaceChatCompletions {
		path = "/v1/chat/completions"
	}
	runtime.forwardSameProtocol(recorder, request, surface, path, "test")
	return recorder
}

func assertCapturedTokens(t *testing.T, event gatewayUsageEvent, input, output uint64, cached, reasoning *uint64) {
	t.Helper()
	if event.Status != "ok" || event.InputTokens != input || event.OutputTokens != output ||
		!sameOptionalUsageUint(event.CachedInputTokens, cached) || !sameOptionalUsageUint(event.ReasoningTokens, reasoning) {
		t.Fatalf("captured usage event=%+v want input=%d output=%d cached=%v reasoning=%v", event, input, output, cached, reasoning)
	}
}

func sameOptionalUsageUint(left, right *uint64) bool {
	if left == nil || right == nil {
		return left == nil && right == nil
	}
	return *left == *right
}

func tokenPointer(value uint64) *uint64 { return &value }

func assertTokenCaptureNoLeak(t *testing.T, runtime *Runtime, spool string) {
	t.Helper()
	for _, root := range []string{spool, runtime.Home()} {
		err := filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
			if walkErr != nil || entry.IsDir() {
				return walkErr
			}
			raw, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			for _, secret := range []string{tokenCaptureIngress, tokenCaptureKey, tokenCaptureBody} {
				if bytes.Contains(raw, []byte(secret)) {
					return fmt.Errorf("token usage capture retained synthetic secret")
				}
			}
			return nil
		})
		if err != nil {
			t.Fatal(err)
		}
	}
}

func TestUsageSpoolCapturesExactNonStreamAndResponsesChatTokens(t *testing.T) {
	tests := []struct {
		name      string
		surface   string
		transport string
		response  map[string]any
		input     uint64
		output    uint64
		cached    *uint64
		reasoning *uint64
	}{
		{
			name: "messages", surface: surfaceMessages, transport: transportAnthropicMessages,
			response: map[string]any{"type": "message", "content": []any{}, "usage": map[string]any{"input_tokens": 13, "output_tokens": 7, "cache_read_input_tokens": 4}},
			input:    13, output: 7, cached: tokenPointer(4),
		},
		{
			name: "responses", surface: surfaceResponses, transport: transportCodexResponses,
			response: map[string]any{
				"object": "response", "status": "completed",
				"usage": map[string]any{
					"input_tokens": 29, "output_tokens": 17,
					"input_tokens_details":  map[string]any{"cached_tokens": 9},
					"output_tokens_details": map[string]any{"reasoning_tokens": 7},
				},
			},
			input: 29, output: 17, cached: tokenPointer(9), reasoning: tokenPointer(7),
		},
		{
			name: "chat", surface: surfaceChatCompletions, transport: transportOpenAIChatCompletions,
			response: map[string]any{
				"object": "chat.completion",
				"usage": map[string]any{
					"prompt_tokens": 31, "completion_tokens": 19,
					"prompt_tokens_details":     map[string]any{"cached_tokens": 10},
					"completion_tokens_details": map[string]any{"reasoning_tokens": 8},
				},
			},
			input: 31, output: 19, cached: tokenPointer(10), reasoning: tokenPointer(8),
		},
		{
			name: "responses_to_chat", surface: surfaceResponses, transport: transportOpenAIChatCompletions,
			response: map[string]any{
				"id": "chatcmpl_token_usage", "object": "chat.completion", "created": 1, "model": tokenCaptureModel,
				"choices": []any{map[string]any{"index": 0, "message": map[string]any{"role": "assistant", "content": "ok"}, "finish_reason": "stop"}},
				"usage": map[string]any{
					"prompt_tokens": 19, "completion_tokens": 11, "total_tokens": 30,
					"prompt_tokens_details":     map[string]any{"cached_tokens": 5},
					"completion_tokens_details": map[string]any{"reasoning_tokens": 3},
				},
			},
			input: 19, output: 11, cached: tokenPointer(5), reasoning: tokenPointer(3),
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				writer.Header().Set("Content-Type", "application/json")
				if test.transport == transportOpenAIChatCompletions && request.URL.Path != "/v1/chat/completions" {
					t.Errorf("converted request path=%q", request.URL.Path)
				}
				_ = json.NewEncoder(writer).Encode(test.response)
			}))
			t.Cleanup(upstream.Close)
			spool := t.TempDir()
			runtime := tokenCaptureRuntime(t, test.surface, test.transport, upstream.URL, spool)
			response := serveTokenCaptureRequest(runtime, test.surface, tokenCaptureRequest(context.Background(), test.surface, false))
			if response.Code != http.StatusOK {
				t.Fatalf("response=%d body=%s", response.Code, response.Body.String())
			}
			events := usageEvents(t, spool)
			if len(events) != 1 {
				t.Fatalf("events=%+v", events)
			}
			assertCapturedTokens(t, events[0], test.input, test.output, test.cached, test.reasoning)
			assertTokenCaptureNoLeak(t, runtime, spool)
		})
	}
}

func TestUsageSpoolCapturesAllSupportedSSETokenShapes(t *testing.T) {
	tests := []struct {
		name      string
		surface   string
		transport string
		stream    string
		input     uint64
		output    uint64
		cached    *uint64
		reasoning *uint64
	}{
		{
			name: "messages", surface: surfaceMessages, transport: transportAnthropicMessages,
			stream: "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":8,\"cache_read_input_tokens\":2}}}\n\n" +
				"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":3}}\n\n" +
				"event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
			input: 8, output: 3, cached: tokenPointer(2),
		},
		{
			name: "responses", surface: surfaceResponses, transport: transportCodexResponses,
			stream: "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":6,\"input_tokens_details\":{\"cached_tokens\":4},\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\n\n",
			input:  10, output: 6, cached: tokenPointer(4), reasoning: tokenPointer(2),
		},
		{
			name: "chat", surface: surfaceChatCompletions, transport: transportOpenAIChatCompletions,
			stream: "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":5,\"prompt_tokens_details\":{\"cached_tokens\":3},\"completion_tokens_details\":{\"reasoning_tokens\":1}}}\n\ndata: [DONE]\n\n",
			input:  9, output: 5, cached: tokenPointer(3), reasoning: tokenPointer(1),
		},
		{
			name: "responses_to_chat", surface: surfaceResponses, transport: transportOpenAIChatCompletions,
			stream: "data: {\"id\":\"chatcmpl_token_stream\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"usage-token-capture-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n" +
				"data: {\"id\":\"chatcmpl_token_stream\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"usage-token-capture-model\",\"choices\":[],\"usage\":{\"prompt_tokens\":15,\"completion_tokens\":8,\"prompt_tokens_details\":{\"cached_tokens\":6},\"completion_tokens_details\":{\"reasoning_tokens\":2}}}\n\n" +
				"data: [DONE]\n\n",
			input: 15, output: 8, cached: tokenPointer(6), reasoning: tokenPointer(2),
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
				writer.Header().Set("Content-Type", "text/event-stream")
				_, _ = writer.Write([]byte(test.stream))
			}))
			t.Cleanup(upstream.Close)
			spool := t.TempDir()
			runtime := tokenCaptureRuntime(t, test.surface, test.transport, upstream.URL, spool)
			response := serveTokenCaptureRequest(runtime, test.surface, tokenCaptureRequest(context.Background(), test.surface, true))
			if response.Code != http.StatusOK {
				t.Fatalf("response=%d body=%s", response.Code, response.Body.String())
			}
			events := usageEvents(t, spool)
			if len(events) != 1 {
				t.Fatalf("events=%+v", events)
			}
			assertCapturedTokens(t, events[0], test.input, test.output, test.cached, test.reasoning)
			assertTokenCaptureNoLeak(t, runtime, spool)
		})
	}
}

func TestUsageSpoolTokenCaptureUsesZerosForMissingOrMalformedUsage(t *testing.T) {
	responses := [][]byte{
		[]byte(`{"type":"message","usage":{"input_tokens":4}}`),
		[]byte(`{"type":"message","usage":{"input_tokens":"bad","output_tokens":2}}`),
	}
	index := 0
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write(responses[index])
		index++
	}))
	t.Cleanup(upstream.Close)
	spool := t.TempDir()
	runtime := tokenCaptureRuntime(t, surfaceMessages, transportAnthropicMessages, upstream.URL, spool)
	for range responses {
		response := serveTokenCaptureRequest(runtime, surfaceMessages, tokenCaptureRequest(context.Background(), surfaceMessages, false))
		if response.Code != http.StatusOK {
			t.Fatalf("response=%d body=%s", response.Code, response.Body.String())
		}
	}
	events := usageEvents(t, spool)
	if len(events) != len(responses) {
		t.Fatalf("events=%+v", events)
	}
	for _, event := range events {
		if event.InputTokens != 0 || event.OutputTokens != 0 || event.CachedInputTokens != nil || event.ReasoningTokens != nil {
			t.Fatalf("missing or malformed usage was not zeroed: %+v", event)
		}
	}
	assertTokenCaptureNoLeak(t, runtime, spool)
}

func TestUsageSpoolClearsObservedTokensAfterStreamFailureOrCancellation(t *testing.T) {
	t.Run("stream_failure", func(t *testing.T) {
		spool := t.TempDir()
		runtime := tokenCaptureRuntime(t, surfaceMessages, transportAnthropicMessages, "http://127.0.0.1:1", spool)
		runtime.upstreamClient = &http.Client{Transport: usageRoundTripper(func(*http.Request) (*http.Response, error) {
			headers := make(http.Header)
			headers.Set("Content-Type", "text/event-stream")
			return &http.Response{
				StatusCode: http.StatusOK,
				Header:     headers,
				Body:       &tokenCaptureUsageThenFailReadCloser{payload: []byte(tokenCaptureMessagesSSE)},
			}, nil
		})}

		response := serveTokenCaptureRequest(runtime, surfaceMessages, tokenCaptureRequest(context.Background(), surfaceMessages, true))
		if response.Code != http.StatusOK {
			t.Fatalf("response=%d body=%s", response.Code, response.Body.String())
		}
		events := usageEvents(t, spool)
		if len(events) != 1 || events[0].Status != "failed" || events[0].ErrorClass == nil || *events[0].ErrorClass != edgeStatusUpstreamUnavailable {
			t.Fatalf("failure event=%+v", events)
		}
		assertCapturedTokensCleared(t, events[0])
		assertTokenCaptureNoLeak(t, runtime, spool)
	})

	t.Run("cancellation", func(t *testing.T) {
		spool := t.TempDir()
		runtime := tokenCaptureRuntime(t, surfaceMessages, transportAnthropicMessages, "http://127.0.0.1:1", spool)
		reader := &tokenCaptureUsageThenBlockReadCloser{
			payload: []byte(tokenCaptureMessagesSSE),
			waiting: make(chan struct{}, 1),
			closed:  make(chan struct{}),
		}
		runtime.upstreamClient = &http.Client{Transport: usageRoundTripper(func(*http.Request) (*http.Response, error) {
			headers := make(http.Header)
			headers.Set("Content-Type", "text/event-stream")
			return &http.Response{StatusCode: http.StatusOK, Header: headers, Body: reader}, nil
		})}

		ctx, cancel := context.WithCancel(context.Background())
		finished := make(chan *httptest.ResponseRecorder, 1)
		go func() {
			finished <- serveTokenCaptureRequest(runtime, surfaceMessages, tokenCaptureRequest(ctx, surfaceMessages, true))
		}()
		select {
		case <-reader.waiting:
		case <-time.After(time.Second):
			t.Fatal("request did not observe valid usage before cancellation")
		}
		cancel()
		select {
		case response := <-finished:
			if response.Code != http.StatusOK {
				t.Fatalf("response=%d body=%s", response.Code, response.Body.String())
			}
		case <-time.After(time.Second):
			t.Fatal("canceled request did not complete")
		}
		events := usageEvents(t, spool)
		if len(events) != 1 || events[0].Status != "failed" || events[0].ErrorClass == nil || *events[0].ErrorClass != edgeStatusRequestCanceled {
			t.Fatalf("cancellation event=%+v", events)
		}
		assertCapturedTokensCleared(t, events[0])
		assertTokenCaptureNoLeak(t, runtime, spool)
	})
}

func assertCapturedTokensCleared(t *testing.T, event gatewayUsageEvent) {
	t.Helper()
	if event.InputTokens != 0 || event.OutputTokens != 0 || event.CachedInputTokens != nil || event.ReasoningTokens != nil {
		t.Fatalf("failed or canceled event retained observed usage: %+v", event)
	}
}
