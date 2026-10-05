package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
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
	usageTestIngress = "ahb_usage_ingress_secret_do_not_store"
	usageTestKey     = "sk_usage_upstream_secret_do_not_store"
	usageTestBody    = "usage-sensitive-body-do-not-store"
	usageTestModel   = "usage-test-model"
)

type usageRoundTripper func(*http.Request) (*http.Response, error)

func (roundTrip usageRoundTripper) RoundTrip(request *http.Request) (*http.Response, error) {
	return roundTrip(request)
}

type usageFailingReadCloser struct{ sent bool }

func (reader *usageFailingReadCloser) Read(buffer []byte) (int, error) {
	if !reader.sent {
		reader.sent = true
		return copy(buffer, "data: first\\n\\n"), nil
	}
	return 0, io.ErrUnexpectedEOF
}

func (*usageFailingReadCloser) Close() error { return nil }

type usageBlockingReadCloser struct {
	started chan struct{}
	closed  chan struct{}
}

func (reader *usageBlockingReadCloser) Read([]byte) (int, error) {
	select {
	case reader.started <- struct{}{}:
	default:
	}
	<-reader.closed
	return 0, io.ErrClosedPipe
}

func (reader *usageBlockingReadCloser) Close() error {
	select {
	case <-reader.closed:
	default:
		close(reader.closed)
	}
	return nil
}

func usageRuntime(t *testing.T, scope, upstream, spool string, members []RuntimeMemberConfig) *Runtime {
	t.Helper()
	if len(members) == 0 {
		members = []RuntimeMemberConfig{usageMember("member-a", "ticket-a", "source-a", upstream, usageTestKey)}
	}
	config := &RuntimeConfig{
		Version:       runtimeConfigVersion,
		UsageSpoolDir: spool,
		Edges: []RuntimeEdgeConfig{{
			ID:             "usage-pool",
			IngressKey:     usageTestIngress,
			Surface:        surfaceMessages,
			Dialect:        "claude",
			SchedulePolicy: policyPriorityFailover,
			FixtureModel:   usageTestModel,
			Members:        members,
		}},
	}
	root, err := os.MkdirTemp("", "ahu-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(root) })
	var runtime *Runtime
	if scope == runtimeScopeProduct {
		home := filepath.Join(root, "runtime", "adapterd")
		if err := os.MkdirAll(home, 0o700); err != nil {
			t.Fatal(err)
		}
		runtime, err = NewRuntimeWithScope(home, productDefaultPort, "", runtimeScopeProduct, func() {})
	} else {
		runtime, err = NewRuntime(root, 0, "", func() {})
	}
	if err != nil {
		t.Fatal(err)
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

func usageMember(id, ticket, sourceID, upstream, key string) RuntimeMemberConfig {
	return RuntimeMemberConfig{
		ID: id, TicketID: ticket, SourceKind: "provider", SourceID: sourceID,
		UpstreamBaseURL: upstream, UpstreamKey: key, UpstreamAuth: authAPIKey,
		UpstreamTransport: transportAnthropicMessages, UpstreamModel: "upstream-usage-model",
		Priority: 0, Position: 0, Models: []string{usageTestModel},
	}
}

func usageRequest(context context.Context, stream bool) *http.Request {
	body := []byte(`{"model":"` + usageTestModel + `","stream":` + map[bool]string{true: "true", false: "false"}[stream] + `,"messages":[{"role":"user","content":"` + usageTestBody + `"}]}`)
	request := httptest.NewRequest(http.MethodPost, "http://127.0.0.1/v1/messages", bytes.NewReader(body)).WithContext(context)
	request.RemoteAddr = "127.0.0.1:12345"
	request.Header.Set("Authorization", "Bearer "+usageTestIngress)
	request.Header.Set("Content-Type", "application/json")
	return request
}

func usageEvents(t *testing.T, spool string) []gatewayUsageEvent {
	t.Helper()
	entries, err := os.ReadDir(spool)
	if err != nil {
		t.Fatal(err)
	}
	var events []gatewayUsageEvent
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasPrefix(entry.Name(), "gateway-") || !strings.HasSuffix(entry.Name(), ".jsonl") {
			continue
		}
		raw, err := os.ReadFile(filepath.Join(spool, entry.Name()))
		if err != nil {
			t.Fatal(err)
		}
		for _, line := range bytes.Split(bytes.TrimSpace(raw), []byte("\n")) {
			var event gatewayUsageEvent
			if err := json.Unmarshal(line, &event); err != nil {
				t.Fatalf("spool row is not GatewayUsageEvent JSON: %v", err)
			}
			events = append(events, event)
		}
	}
	if len(events) == 0 {
		t.Fatal("usage spool has no events")
	}
	return events
}

func serveUsageRequest(runtime *Runtime, request *http.Request) *httptest.ResponseRecorder {
	recorder := httptest.NewRecorder()
	runtime.forwardSameProtocol(recorder, request, surfaceMessages, "/v1/messages", "test")
	return recorder
}

func TestUsageSpoolRecordsSuccessAndFinalHTTPFailure(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		if strings.Contains(request.URL.RawQuery, "fail") {
			writer.WriteHeader(http.StatusBadGateway)
			return
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"ok":true}`))
	}))
	t.Cleanup(upstream.Close)
	spool := t.TempDir()
	runtime := usageRuntime(t, runtimeScopeProduct, upstream.URL, spool, nil)

	if response := serveUsageRequest(runtime, usageRequest(context.Background(), false)); response.Code != http.StatusOK {
		t.Fatalf("success response=%d", response.Code)
	}
	runtime.upstreamClient = &http.Client{Transport: usageRoundTripper(func(_ *http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: http.StatusBadGateway, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"error":"ignored"}`))}, nil
	})}
	if response := serveUsageRequest(runtime, usageRequest(context.Background(), false)); response.Code != http.StatusBadGateway {
		t.Fatalf("failure response=%d", response.Code)
	}

	events := usageEvents(t, spool)
	if len(events) != 2 || events[0].Status != "ok" || events[1].Status != "failed" {
		t.Fatalf("events=%+v", events)
	}
	for _, event := range events {
		if event.ProfileID != "usage-pool" || event.Surface != "messages" || event.Model == nil || *event.Model != usageTestModel || event.TicketID == nil || *event.TicketID != "ticket-a" || event.AccountKind == nil || *event.AccountKind != "provider" || event.AccountID == nil || *event.AccountID != "source-a" || event.UpstreamModel == nil || *event.UpstreamModel != "upstream-usage-model" || event.UpstreamChannel == nil || *event.UpstreamChannel != "anthropic" || event.Attempts == nil || *event.Attempts != 1 {
			t.Fatalf("incomplete usage event: %+v", event)
		}
		if _, err := time.Parse(time.RFC3339Nano, event.Timestamp); err != nil || !strings.HasSuffix(event.Timestamp, "Z") {
			t.Fatalf("timestamp=%q err=%v", event.Timestamp, err)
		}
		if len(event.RequestID) != 36 || event.RequestID[14] != '4' {
			t.Fatalf("request id is not UUIDv4: %q", event.RequestID)
		}
	}
	if events[1].ErrorClass == nil || *events[1].ErrorClass != edgeStatusUpstreamUnavailable || events[1].StatusCode == nil || *events[1].StatusCode != http.StatusBadGateway {
		t.Fatalf("failure event=%+v", events[1])
	}
}

func TestUsageSpoolRecordsSSEFailureAndCancellation(t *testing.T) {
	spool := t.TempDir()
	runtime := usageRuntime(t, runtimeScopeProduct, "http://127.0.0.1:18080", spool, nil)
	runtime.upstreamClient = &http.Client{Transport: usageRoundTripper(func(_ *http.Request) (*http.Response, error) {
		headers := make(http.Header)
		headers.Set("Content-Type", "text/event-stream")
		return &http.Response{StatusCode: http.StatusOK, Header: headers, Body: &usageFailingReadCloser{}}, nil
	})}
	if response := serveUsageRequest(runtime, usageRequest(context.Background(), true)); response.Code != http.StatusOK {
		t.Fatalf("SSE response=%d", response.Code)
	}

	canceled, cancel := context.WithCancel(context.Background())
	blocking := &usageBlockingReadCloser{started: make(chan struct{}, 1), closed: make(chan struct{})}
	canceledRuntime := usageRuntime(t, runtimeScopeProduct, "http://127.0.0.1:18080", spool, nil)
	canceledRuntime.upstreamClient = &http.Client{Transport: usageRoundTripper(func(_ *http.Request) (*http.Response, error) {
		headers := make(http.Header)
		headers.Set("Content-Type", "text/event-stream")
		return &http.Response{StatusCode: http.StatusOK, Header: headers, Body: blocking}, nil
	})}
	finished := make(chan *httptest.ResponseRecorder, 1)
	go func() { finished <- serveUsageRequest(canceledRuntime, usageRequest(canceled, true)) }()
	select {
	case <-blocking.started:
	case <-time.After(time.Second):
		t.Fatal("request did not enter routing before cancellation")
	}
	cancel()
	select {
	case <-finished:
	case <-time.After(time.Second):
		t.Fatal("canceled request did not complete")
	}
	events := usageEvents(t, spool)
	if len(events) != 2 || events[0].Status != "failed" || events[0].TTFTMS == nil || events[0].ErrorClass == nil || *events[0].ErrorClass != edgeStatusUpstreamUnavailable {
		t.Fatalf("SSE failure event=%+v", events)
	}
	if events[1].Status != "failed" || events[1].ErrorClass == nil || *events[1].ErrorClass != edgeStatusRequestCanceled || events[1].Attempts == nil || *events[1].Attempts != 1 {
		t.Fatalf("cancellation event=%+v", events[1])
	}
	for _, event := range events {
		if event.InputTokens != 0 || event.OutputTokens != 0 || event.CachedInputTokens != nil || event.ReasoningTokens != nil {
			t.Fatalf("failed or canceled event retained usage: %+v", event)
		}
	}
}

func TestUsageSpoolRecordsFinalRetryAndPreservesWriterThroughHotReload(t *testing.T) {
	var attempts int
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		attempts++
		if request.Header.Get("X-API-Key") == "retry-first-secret" {
			writer.WriteHeader(http.StatusBadGateway)
			return
		}
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"usage":{"input_tokens":9,"output_tokens":2}}`))
	}))
	t.Cleanup(upstream.Close)
	spool := t.TempDir()
	members := []RuntimeMemberConfig{
		usageMember("first", "ticket-first", "source-first", upstream.URL, "retry-first-secret"),
		usageMember("second", "ticket-final", "source-final", upstream.URL, "retry-final-secret"),
	}
	runtime := usageRuntime(t, runtimeScopeProduct, upstream.URL, spool, members)
	if response := serveUsageRequest(runtime, usageRequest(context.Background(), false)); response.Code != http.StatusOK {
		t.Fatalf("retry response=%d", response.Code)
	}
	events := usageEvents(t, spool)
	if len(events) != 1 || events[0].Attempts == nil || *events[0].Attempts != 2 || events[0].TicketID == nil || *events[0].TicketID != "ticket-final" || events[0].AccountID == nil || *events[0].AccountID != "source-final" || attempts != 2 {
		t.Fatalf("retry event=%+v attempts=%d", events, attempts)
	}
	if events[0].InputTokens != 9 || events[0].OutputTokens != 2 {
		t.Fatalf("retry token event=%+v", events[0])
	}

	runtime.mu.Lock()
	writerBefore := runtime.usageSpool
	runtime.mu.Unlock()
	updated := &RuntimeConfig{Version: runtimeConfigVersion, UsageSpoolDir: spool, Edges: []RuntimeEdgeConfig{{
		ID: "usage-pool", IngressKey: usageTestIngress, Surface: surfaceMessages, Dialect: "claude", SchedulePolicy: policyPriorityFailover, FixtureModel: usageTestModel,
		Members: []RuntimeMemberConfig{usageMember("updated", "ticket-updated", "source-updated", upstream.URL, "retry-final-secret")},
	}}}
	if err := runtime.SwapRuntimeConfig(updated, "usage-hot-reload"); err != nil {
		t.Fatal(err)
	}
	runtime.mu.Lock()
	writerAfter := runtime.usageSpool
	runtime.mu.Unlock()
	if writerBefore == nil || writerBefore != writerAfter {
		t.Fatal("same spool directory did not preserve its single writer on hot reload")
	}
	if response := serveUsageRequest(runtime, usageRequest(context.Background(), false)); response.Code != http.StatusOK {
		t.Fatalf("post-reload response=%d", response.Code)
	}
	if got := len(usageEvents(t, spool)); got != 2 {
		t.Fatalf("events after reload=%d", got)
	}
}

func TestUsageSpoolIsDisabledWithoutProductConfigAndNeverStoresSecrets(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, _ *http.Request) {
		writer.Header().Set("Content-Type", "application/json")
		_, _ = writer.Write([]byte(`{"ok":true}`))
	}))
	t.Cleanup(upstream.Close)
	disabled := usageRuntime(t, runtimeScopeIsolated, upstream.URL, "", nil)
	disabled.mu.Lock()
	spool := disabled.usageSpool
	disabled.mu.Unlock()
	if spool != nil {
		t.Fatal("isolated config without usage_spool_dir enabled capture")
	}
	if response := serveUsageRequest(disabled, usageRequest(context.Background(), false)); response.Code != http.StatusOK {
		t.Fatalf("disabled response=%d", response.Code)
	}

	enabledDir := t.TempDir()
	isolatedWithSpool := &RuntimeConfig{
		Version: runtimeConfigVersion, UsageSpoolDir: enabledDir,
		Edges: []RuntimeEdgeConfig{{
			ID: "isolated-with-spool", IngressKey: usageTestIngress, Surface: surfaceMessages, Dialect: "claude", SchedulePolicy: policyPriorityFailover, FixtureModel: usageTestModel,
			Members: []RuntimeMemberConfig{usageMember("isolated", "ticket-isolated", "source-isolated", upstream.URL, usageTestKey)},
		}},
	}
	if err := disabled.SetRuntimeConfig(isolatedWithSpool); err == nil || !strings.Contains(err.Error(), "Product-only") {
		t.Fatalf("isolated runtime accepted spool config: %v", err)
	}
	if entries, err := os.ReadDir(enabledDir); err != nil || len(entries) != 0 {
		t.Fatalf("isolated spool config created files: entries=%v err=%v", entries, err)
	}

	enabled := usageRuntime(t, runtimeScopeProduct, upstream.URL, enabledDir, nil)
	if response := serveUsageRequest(enabled, usageRequest(context.Background(), false)); response.Code != http.StatusOK {
		t.Fatalf("enabled response=%d", response.Code)
	}
	_ = usageEvents(t, enabledDir)
	for _, root := range []string{enabledDir, enabled.Home()} {
		err := filepath.WalkDir(root, func(path string, entry os.DirEntry, walkErr error) error {
			if walkErr != nil || entry.IsDir() {
				return walkErr
			}
			raw, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			for _, secret := range []string{usageTestIngress, usageTestKey, usageTestBody, upstream.URL} {
				if bytes.Contains(raw, []byte(secret)) {
					return errors.New("usage spool or sidecar log retained a synthetic secret")
				}
			}
			return nil
		})
		if err != nil {
			t.Fatal(err)
		}
	}
}
