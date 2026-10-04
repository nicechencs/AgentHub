package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

const (
	safetyIngressKey = "ahb_http_safety_ingress_synthetic"
	safetyModel      = "claude-http-safety"
	safetySecret     = "upstream-http-safety-secret-must-not-escape"
)

func safetyMessagesConfig(upstream string) *RuntimeConfig {
	return &RuntimeConfig{
		Version: runtimeConfigVersion,
		Edges: []RuntimeEdgeConfig{{
			ID: "http-safety", IngressKey: safetyIngressKey, Surface: surfaceMessages,
			Dialect: "claude", SchedulePolicy: policyPriorityFailover, FixtureModel: safetyModel,
			Members: []RuntimeMemberConfig{{
				ID: "http-safety-member", UpstreamBaseURL: upstream, UpstreamKey: "upstream-http-safety-key",
				UpstreamAuth: authAPIKey, UpstreamTransport: transportAnthropicMessages, Models: []string{safetyModel},
			}},
		}},
	}
}

func safetyResponsesChatConfig(upstream string) *RuntimeConfig {
	return &RuntimeConfig{
		Version: runtimeConfigVersion,
		Edges: []RuntimeEdgeConfig{{
			ID: "http-safety-responses", IngressKey: safetyIngressKey, Surface: surfaceResponses,
			Dialect: "codex", SchedulePolicy: policyPriorityFailover, FixtureModel: safetyModel,
			Members: []RuntimeMemberConfig{{
				ID: "http-safety-chat-member", UpstreamBaseURL: upstream, UpstreamKey: "upstream-http-safety-key",
				UpstreamAuth: authBearer, UpstreamTransport: transportOpenAIChatCompletions, Models: []string{safetyModel},
			}},
		}},
	}
}

func safetyPOST(t *testing.T, port int, body io.Reader) (*http.Response, error) {
	t.Helper()
	req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", body)
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer "+safetyIngressKey)
	req.Header.Set("Content-Type", "application/json")
	return http.DefaultClient.Do(req)
}

func safetyBody(stream bool) []byte {
	return []byte(`{"model":"` + safetyModel + `","stream":` + strconv.FormatBool(stream) + `,"messages":[{"role":"user","content":"ping"}]}`)
}

func TestHTTPStrictIngressLimitRejectsBeforeUpstream(t *testing.T) {
	var hits atomic.Int32
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		hits.Add(1)
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"ok":true}`)
	}))
	t.Cleanup(upstream.Close)
	_, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))

	body := bytes.Repeat([]byte("x"), int(defaultRouteHTTPSafetyPolicy.IngressBodyBytes)+1)
	req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.ContentLength = -1 // Exercise the read-the-extra-byte path, not only the declared length shortcut.
	req.Header.Set("Authorization", "Bearer "+safetyIngressKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusRequestEntityTooLarge || !bytes.Contains(got, []byte("request_too_large")) {
		t.Fatalf("status=%d body=%s", resp.StatusCode, got)
	}
	if hits.Load() != 0 {
		t.Fatalf("oversize request reached upstream %d times", hits.Load())
	}
}

func TestHTTPStrictControlLimitRejectsBeforeDispatch(t *testing.T) {
	rt := testRuntime(t)
	raw := bytes.Repeat([]byte("x"), int(defaultRouteHTTPSafetyPolicy.ControlBodyBytes)+1)
	req := httptest.NewRequest(http.MethodPost, "http://agenthub.local/control", bytes.NewReader(raw))
	recorder := httptest.NewRecorder()
	rt.serveControlHTTP(recorder, req)
	if recorder.Code != http.StatusRequestEntityTooLarge {
		t.Fatalf("status=%d body=%s", recorder.Code, recorder.Body.String())
	}
	if rt.handshaked {
		t.Fatal("oversize control request reached the control dispatcher")
	}
}

type zeroReader struct{}

func (zeroReader) Read(p []byte) (int, error) {
	for i := range p {
		p[i] = 0
	}
	return len(p), nil
}

type deadlineRecorder struct {
	*httptest.ResponseRecorder
	deadlines []time.Time
}

func (r *deadlineRecorder) SetWriteDeadline(deadline time.Time) error {
	r.deadlines = append(r.deadlines, deadline)
	return nil
}

func TestHTTPDownstreamWriteDeadlineIsRefreshed(t *testing.T) {
	recorder := &deadlineRecorder{ResponseRecorder: httptest.NewRecorder()}
	before := time.Now()
	refreshDownstreamWriteDeadline(recorder, 250*time.Millisecond)
	if len(recorder.deadlines) != 1 {
		t.Fatalf("deadlines=%v", recorder.deadlines)
	}
	if got := recorder.deadlines[0]; got.Before(before.Add(200*time.Millisecond)) || got.After(time.Now().Add(300*time.Millisecond)) {
		t.Fatalf("write deadline=%s", got)
	}
}

func TestHTTPNonStreamResponseLimitDetectsThirtyTwoMiBPlusOne(t *testing.T) {
	limit := defaultRouteHTTPSafetyPolicy.NonStreamBodyBytes
	resp := &http.Response{ContentLength: -1, Body: io.NopCloser(io.LimitReader(zeroReader{}, limit+1))}
	if _, err := readBoundedResponseBody(context.Background(), resp, limit, time.Second, time.Second); !errors.Is(err, errUpstreamBodyTooLarge) {
		t.Fatalf("error=%v", err)
	}
}

func TestHTTPNonStreamBodyIdleAndTotalTimeoutsReleaseSlot(t *testing.T) {
	tests := []struct {
		name       string
		writeDelay time.Duration
	}{
		{name: "header-then-stall"},
		{name: "slow-drip-hits-total-timeout", writeDelay: 20 * time.Millisecond},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var hits atomic.Int32
			upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				if hits.Add(1) > 1 {
					w.Header().Set("Content-Type", "application/json")
					_, _ = io.WriteString(w, `{"ok":true}`)
					return
				}
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusOK)
				if flusher, ok := w.(http.Flusher); ok {
					flusher.Flush()
				}
				if tt.writeDelay == 0 {
					<-r.Context().Done()
					return
				}
				ticker := time.NewTicker(tt.writeDelay)
				defer ticker.Stop()
				for {
					select {
					case <-r.Context().Done():
						return
					case <-ticker.C:
						if _, err := io.WriteString(w, " "); err != nil {
							return
						}
						if flusher, ok := w.(http.Flusher); ok {
							flusher.Flush()
						}
					}
				}
			}))
			t.Cleanup(upstream.Close)
			rt, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))
			rt.requestSlots = make(chan struct{}, 1)
			rt.httpPolicy.NonStreamIdleTimeout = 50 * time.Millisecond
			rt.httpPolicy.NonStreamTotalTimeout = 140 * time.Millisecond

			started := time.Now()
			resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(false)))
			if err != nil {
				t.Fatal(err)
			}
			body, _ := io.ReadAll(resp.Body)
			_ = resp.Body.Close()
			if resp.StatusCode != http.StatusBadGateway || !bytes.Contains(body, []byte(`"code":"upstream_error"`)) || time.Since(started) > time.Second {
				t.Fatalf("status=%d elapsed=%s body=%s", resp.StatusCode, time.Since(started), body)
			}

			reuseBody := []byte(`{"model":"not-in-the-pool","stream":false}`)
			reused, err := safetyPOST(t, port, bytes.NewReader(reuseBody))
			if err != nil {
				t.Fatal(err)
			}
			reusedBody, _ := io.ReadAll(reused.Body)
			_ = reused.Body.Close()
			if reused.StatusCode != http.StatusNotFound || !bytes.Contains(reusedBody, []byte(`"code":"model_not_found"`)) {
				t.Fatalf("slot was not reusable: status=%d body=%s", reused.StatusCode, reusedBody)
			}
		})
	}
}

func TestHTTPConvertedResponsesRechecksExpandedJSONLimit(t *testing.T) {
	const limit = int64(1024)
	var upstreamBody []byte
	for count := 1; count < int(limit); count++ {
		candidate, err := json.Marshal(map[string]any{
			"id": "chatcmpl_expand", "model": safetyModel, "created": 7,
			"choices": []any{map[string]any{
				"message":       map[string]any{"role": "assistant", "content": strings.Repeat("<", count)},
				"finish_reason": "stop",
			}},
		})
		if err != nil {
			t.Fatal(err)
		}
		translated, err := translateChatJSONToResponses(candidate)
		if err != nil {
			t.Fatal(err)
		}
		if int64(len(candidate)) <= limit && int64(len(translated)) > limit {
			upstreamBody = candidate
			break
		}
	}
	if upstreamBody == nil {
		t.Fatal("could not construct a JSON-escaping expansion fixture")
	}

	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(upstreamBody)
	}))
	t.Cleanup(upstream.Close)
	rt, _, _, port := configuredRuntime(t, safetyResponsesChatConfig(upstream.URL))
	rt.httpPolicy.NonStreamBodyBytes = limit
	body, err := json.Marshal(map[string]any{"model": safetyModel, "input": "ping"})
	if err != nil {
		t.Fatal(err)
	}
	req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/responses", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer "+safetyIngressKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusBadGateway || !bytes.Equal(got, safeUpstreamErrorBody()) || int64(len(got)) > limit {
		t.Fatalf("status=%d bytes=%d body=%s", resp.StatusCode, len(got), got)
	}
}

func TestHTTPUpstreamErrorsAndHeadersAreSanitized(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "text/plain")
		w.Header().Set("Retry-After", "120")
		w.Header().Set("Set-Cookie", "session="+safetySecret)
		w.Header().Set("Location", "https://evil.invalid/"+safetySecret)
		w.Header().Set("WWW-Authenticate", "Bearer "+safetySecret)
		w.WriteHeader(http.StatusBadRequest)
		_, _ = io.WriteString(w, safetySecret)
	}))
	t.Cleanup(upstream.Close)
	_, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))

	resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(false)))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusBadRequest || !bytes.Contains(body, []byte(`"code":"upstream_error"`)) || bytes.Contains(body, []byte(safetySecret)) {
		t.Fatalf("status=%d body=%s", resp.StatusCode, body)
	}
	if resp.Header.Get("Retry-After") != "120" {
		t.Fatalf("Retry-After=%q", resp.Header.Get("Retry-After"))
	}
	for _, name := range []string{"Set-Cookie", "Location", "WWW-Authenticate", "Authorization", "Connection"} {
		if value := resp.Header.Get(name); value != "" {
			t.Fatalf("unsafe header %s=%q", name, value)
		}
	}
}

func TestHTTPRedirectIsNotFollowedOrExposed(t *testing.T) {
	var targetHits atomic.Int32
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		targetHits.Add(1)
		w.WriteHeader(http.StatusNoContent)
	}))
	t.Cleanup(target.Close)
	redirect := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Location", target.URL+"/"+safetySecret)
		w.WriteHeader(http.StatusTemporaryRedirect)
		_, _ = io.WriteString(w, safetySecret)
	}))
	t.Cleanup(redirect.Close)
	_, _, _, port := configuredRuntime(t, safetyMessagesConfig(redirect.URL))

	resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(false)))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusTemporaryRedirect || resp.Header.Get("Location") != "" || bytes.Contains(body, []byte(safetySecret)) {
		t.Fatalf("status=%d location=%q body=%s", resp.StatusCode, resp.Header.Get("Location"), body)
	}
	if targetHits.Load() != 0 {
		t.Fatal("gateway followed the upstream redirect")
	}
}

func TestHTTPNonStreamOversizeAndMalformedResponsesAreSafe(t *testing.T) {
	tests := []struct {
		name    string
		handler http.HandlerFunc
	}{
		{
			name: "oversize",
			handler: func(w http.ResponseWriter, _ *http.Request) {
				w.Header().Set("Content-Type", "application/json")
				w.Header().Set("Content-Length", strconv.FormatInt(defaultRouteHTTPSafetyPolicy.NonStreamBodyBytes+1, 10))
				w.WriteHeader(http.StatusOK)
				_, _ = io.WriteString(w, `{`+safetySecret)
			},
		},
		{
			name: "malformed-json",
			handler: func(w http.ResponseWriter, _ *http.Request) {
				w.Header().Set("Content-Type", "application/json")
				_, _ = io.WriteString(w, `{`+safetySecret)
			},
		},
		{
			name: "wrong-content-type",
			handler: func(w http.ResponseWriter, _ *http.Request) {
				w.Header().Set("Content-Type", "text/plain")
				_, _ = io.WriteString(w, safetySecret)
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			upstream := httptest.NewServer(tt.handler)
			t.Cleanup(upstream.Close)
			_, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))
			resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(false)))
			if err != nil {
				t.Fatal(err)
			}
			defer resp.Body.Close()
			body, _ := io.ReadAll(resp.Body)
			if resp.StatusCode != http.StatusBadGateway || !bytes.Contains(body, []byte(`"code":"upstream_error"`)) || bytes.Contains(body, []byte(safetySecret)) {
				t.Fatalf("status=%d body=%s", resp.StatusCode, body)
			}
		})
	}
}

func TestHTTPSSELimitsIdleTypeAndHeaders(t *testing.T) {
	tests := []struct {
		name    string
		handler http.HandlerFunc
	}{
		{
			name: "oversize",
			handler: func(w http.ResponseWriter, _ *http.Request) {
				w.Header().Set("Content-Type", "text/event-stream")
				_, _ = io.WriteString(w, strings.Repeat("x", 65)+safetySecret)
			},
		},
		{
			name: "idle",
			handler: func(w http.ResponseWriter, r *http.Request) {
				w.Header().Set("Content-Type", "text/event-stream")
				w.WriteHeader(http.StatusOK)
				if flusher, ok := w.(http.Flusher); ok {
					flusher.Flush()
				}
				<-r.Context().Done()
			},
		},
		{
			name: "wrong-content-type",
			handler: func(w http.ResponseWriter, _ *http.Request) {
				w.Header().Set("Content-Type", "application/json")
				_, _ = io.WriteString(w, `{"secret":"`+safetySecret+`"}`)
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			upstream := httptest.NewServer(tt.handler)
			t.Cleanup(upstream.Close)
			rt, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))
			rt.httpPolicy.SSEBodyBytes = 64
			rt.httpPolicy.SSEIdleTimeout = 50 * time.Millisecond
			resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(true)))
			if err != nil {
				t.Fatal(err)
			}
			defer resp.Body.Close()
			body, _ := io.ReadAll(resp.Body)
			if resp.StatusCode != http.StatusBadGateway || !bytes.Contains(body, []byte(`"code":"upstream_error"`)) || bytes.Contains(body, []byte(safetySecret)) {
				t.Fatalf("status=%d body=%s", resp.StatusCode, body)
			}
		})
	}

	t.Run("success-header-allowlist", func(t *testing.T) {
		upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			w.Header().Set("Content-Type", "text/event-stream; charset=utf-8")
			w.Header().Set("Set-Cookie", "session="+safetySecret)
			w.Header().Set("Location", "https://evil.invalid/"+safetySecret)
			_, _ = io.WriteString(w, "data: {\"ok\":true}\n\n")
		}))
		t.Cleanup(upstream.Close)
		_, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))
		resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(true)))
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		body, _ := io.ReadAll(resp.Body)
		if resp.StatusCode != http.StatusOK || !bytes.Contains(body, []byte(`"ok":true`)) {
			t.Fatalf("status=%d body=%s", resp.StatusCode, body)
		}
		if resp.Header.Get("Set-Cookie") != "" || resp.Header.Get("Location") != "" || resp.Header.Get("Content-Type") != "text/event-stream" {
			t.Fatalf("headers=%v", resp.Header)
		}
	})
}

func TestHTTPConcurrencyLimitAndCancellationReleaseSlot(t *testing.T) {
	entered := make(chan struct{}, 1)
	releaseUpstream := make(chan struct{})
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		if flusher, ok := w.(http.Flusher); ok {
			flusher.Flush()
		}
		entered <- struct{}{}
		select {
		case <-r.Context().Done():
		case <-releaseUpstream:
		}
	}))
	t.Cleanup(upstream.Close)
	t.Cleanup(func() { close(releaseUpstream) })
	rt, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))
	rt.requestSlots = make(chan struct{}, 1)

	ctx, cancel := context.WithCancel(context.Background())
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", bytes.NewReader(safetyBody(false)))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer "+safetyIngressKey)
	firstDone := make(chan error, 1)
	go func() {
		resp, doErr := http.DefaultClient.Do(req)
		if resp != nil {
			_ = resp.Body.Close()
		}
		firstDone <- doErr
	}()
	select {
	case <-entered:
	case <-time.After(time.Second):
		t.Fatal("first request did not reach upstream")
	}

	started := time.Now()
	second, err := safetyPOST(t, port, bytes.NewReader(safetyBody(false)))
	if err != nil {
		t.Fatal(err)
	}
	secondBody, _ := io.ReadAll(second.Body)
	_ = second.Body.Close()
	if second.StatusCode != http.StatusServiceUnavailable || !bytes.Contains(secondBody, []byte("route_busy")) || time.Since(started) > time.Second {
		t.Fatalf("status=%d elapsed=%s body=%s", second.StatusCode, time.Since(started), secondBody)
	}

	cancel()
	select {
	case <-firstDone:
	case <-time.After(time.Second):
		t.Fatal("cancelled request did not return")
	}
	deadline := time.Now().Add(time.Second)
	for {
		rt.mu.Lock()
		inFlight := rt.inFlight
		rt.mu.Unlock()
		if len(rt.requestSlots) == 0 && inFlight == 0 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatalf("slot not released: slots=%d in_flight=%d", len(rt.requestSlots), inFlight)
		}
		time.Sleep(10 * time.Millisecond)
	}
}

func TestHTTPUpstreamHeaderTimeoutReturnsSafeFailure(t *testing.T) {
	releaseUpstream := make(chan struct{})
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
		case <-releaseUpstream:
		}
	}))
	t.Cleanup(upstream.Close)
	t.Cleanup(func() { close(releaseUpstream) })
	rt, _, _, port := configuredRuntime(t, safetyMessagesConfig(upstream.URL))
	policy := defaultRouteHTTPSafetyPolicy
	policy.UpstreamHeaderTimeout = 50 * time.Millisecond
	rt.upstreamClient = newUpstreamHTTPClientWithPolicy(policy)

	started := time.Now()
	resp, err := safetyPOST(t, port, bytes.NewReader(safetyBody(false)))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	body, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusBadGateway || !bytes.Contains(body, []byte(`"code":"upstream_error"`)) || time.Since(started) > time.Second {
		t.Fatalf("status=%d elapsed=%s body=%s", resp.StatusCode, time.Since(started), body)
	}
}

func TestHTTPUpstreamClientSharesBoundedTransportPolicy(t *testing.T) {
	if newUpstreamHTTPClient() != newUpstreamHTTPClient() {
		t.Fatal("production upstream client is not shared")
	}
	transport, ok := newUpstreamHTTPClient().Transport.(*http.Transport)
	if !ok {
		t.Fatalf("transport type=%T", newUpstreamHTTPClient().Transport)
	}
	policy := defaultRouteHTTPSafetyPolicy
	if transport.DialContext == nil || transport.TLSHandshakeTimeout != policy.UpstreamTLSHandshake ||
		transport.ResponseHeaderTimeout != policy.UpstreamHeaderTimeout || transport.IdleConnTimeout != policy.UpstreamIdleTimeout ||
		transport.MaxConnsPerHost != policy.UpstreamMaxConns || transport.MaxIdleConnsPerHost != policy.UpstreamMaxConns ||
		transport.MaxResponseHeaderBytes != int64(policy.MaxHeaderBytes) || newUpstreamHTTPClient().CheckRedirect == nil ||
		newUpstreamHTTPClient().Timeout != 0 {
		t.Fatalf("upstream transport does not match policy: %+v", transport)
	}
}

func TestHTTPFinalURLValidationDoesNotExpandAllowlist(t *testing.T) {
	allowed := []struct {
		url       string
		transport string
	}{
		{"http://127.0.0.1:18080/v1/messages", transportAnthropicMessages},
		{"http://[::1]:18080/v1/responses", transportCodexResponses},
		{"https://api.anthropic.com/v1/messages", transportAnthropicMessages},
		{"https://api.anthropic.com:443/v1/messages", transportAnthropicMessages},
	}
	for _, item := range allowed {
		if err := validateFinalUpstreamURL(item.url, item.transport); err != nil {
			t.Errorf("allowed final URL %q: %v", item.url, err)
		}
	}
	denied := []struct {
		url       string
		transport string
	}{
		{"https://api.anthropic.com/v1/responses", transportAnthropicMessages},
		{"https://api.anthropic.com/v1/messages?key=secret", transportAnthropicMessages},
		{"https://api.anthropic.com.evil.invalid/v1/messages", transportAnthropicMessages},
		{"https://chatgpt.com/backend-api/codex/responses", transportCodexResponses},
		{"file:///tmp/socket", transportAnthropicMessages},
	}
	for _, item := range denied {
		if err := validateFinalUpstreamURL(item.url, item.transport); err == nil {
			t.Errorf("denied final URL accepted: %q", item.url)
		}
	}
}
