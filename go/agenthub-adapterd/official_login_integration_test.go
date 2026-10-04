package main

import (
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

const officialIntegrationIngress = "ahb_official_integration_ingress"

func officialRuntimeConfig(target string) *RuntimeConfig {
	member := RuntimeMemberConfig{
		ID:              "account:official",
		SourceKind:      "account",
		SourceID:        "official",
		UpstreamKey:     "synthetic-official-access",
		UpstreamAuth:    authBearer,
		CredentialClass: credentialClassOfficialLogin,
		Priority:        0,
		Position:        0,
		Models:          []string{"gpt-5-codex"},
	}
	dialect := "codex"
	switch target {
	case upstreamTargetCodexChatGPTSubscription:
		member.RefreshKind = refreshCodexOAuth
		member.UpstreamBaseURL = "https://chatgpt.com/backend-api/codex"
		member.UpstreamTransport = transportCodexResponses
		member.UpstreamTarget = target
		member.OfficialAccountID = "acct_synthetic"
	case upstreamTargetGrokXAISubscription:
		dialect = "grok"
		member.RefreshKind = refreshGrokOAuth
		member.UpstreamBaseURL = "https://cli-chat-proxy.grok.com/v1"
		member.UpstreamTransport = transportGrokResponses
		member.UpstreamTarget = target
	}
	return &RuntimeConfig{Version: runtimeConfigVersion, Edges: []RuntimeEdgeConfig{{
		ID: "official-edge", IngressKey: officialIntegrationIngress,
		Surface: surfaceResponses, Dialect: dialect, SchedulePolicy: policyPriorityFailover,
		FixtureModel: "gpt-5-codex", Members: []RuntimeMemberConfig{member},
	}}}
}

func servingOfficialRuntime(t *testing.T, config *RuntimeConfig, transport http.RoundTripper) *Runtime {
	t.Helper()
	rt := testRuntime(t)
	if err := rt.SetRuntimeConfig(config); err != nil {
		t.Fatal(err)
	}
	rt.upstreamClient = &http.Client{Transport: transport}
	rt.mu.Lock()
	rt.lifecycle = lifecycleServing
	rt.listenReady = true
	rt.ownerTerm = 1
	rt.ownerLeaseUntil = time.Now().Add(time.Minute)
	rt.mu.Unlock()
	return rt
}

func officialResponsesRequest(body string) *http.Request {
	req := httptest.NewRequest(http.MethodPost, "http://127.0.0.1/v1/responses", strings.NewReader(body))
	req.RemoteAddr = "127.0.0.1:12345"
	req.Header.Set("Authorization", "Bearer "+officialIntegrationIngress)
	req.Header.Set("Content-Type", "application/json")
	return req
}

func TestOfficialCodexNonStreamUsesFixedIdentityAndAggregatesSSE(t *testing.T) {
	transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
		if got, want := req.URL.String(), "https://chatgpt.com/backend-api/codex/responses"; got != want {
			t.Fatalf("URL=%q want=%q", got, want)
		}
		for name, want := range map[string]string{
			"Authorization":      "Bearer synthetic-official-access",
			"ChatGPT-Account-ID": "acct_synthetic",
			"Accept":             "text/event-stream",
			"OpenAI-Beta":        "responses=experimental",
			"Originator":         "codex-tui",
			"Version":            "0.146.0",
			"User-Agent":         "codex-tui/0.146.0",
		} {
			if got := req.Header.Get(name); got != want {
				t.Errorf("%s=%q want=%q", name, got, want)
			}
		}
		raw, _ := io.ReadAll(req.Body)
		var body map[string]any
		if json.Unmarshal(raw, &body) != nil || body["stream"] != true || body["store"] != false || body["max_output_tokens"] != nil {
			t.Fatalf("prepared body=%s", raw)
		}
		stream := "event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-codex\"}}\n\n" +
			"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"sequence_number\":1,\"delta\":\"hello\"}\n\n" +
			"event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":2,\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-5-codex\",\"status\":\"completed\",\"output\":[]}}\n\n"
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetCodexChatGPTSubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":false,"max_output_tokens":99}`))
	if recorder.Code != http.StatusOK || !strings.Contains(recorder.Body.String(), "hello") {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
	if got := recorder.Header().Get("Content-Type"); !strings.HasPrefix(got, "application/json") {
		t.Fatalf("content type=%q", got)
	}
}

func TestOfficialCodexRejectsMalformedSSEBeforeCommit(t *testing.T) {
	transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
		stream := "event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":1}\n\n"
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetCodexChatGPTSubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":false}`))
	if recorder.Code != http.StatusBadGateway || strings.Contains(recorder.Body.String(), "sequence_number") {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialGrokUsesCLIIdentityWithoutInboundSecrets(t *testing.T) {
	transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
		if got, want := req.URL.String(), "https://cli-chat-proxy.grok.com/v1/responses"; got != want {
			t.Fatalf("URL=%q want=%q", got, want)
		}
		if got := req.Header.Get("Authorization"); got != "Bearer synthetic-official-access" {
			t.Errorf("Authorization=%q", got)
		}
		if req.Header.Get("Cookie") != "" || req.Header.Get("Proxy-Authorization") != "" || !strings.Contains(req.Header.Get("User-Agent"), "grok-shell/") {
			t.Errorf("unsafe or missing headers=%#v", req.Header)
		}
		for _, name := range []string{"X-Xai-Token-Auth", "X-Grok-Agent-Id", "X-Grok-Req-Id", "Traceparent"} {
			if req.Header.Get(name) == "" {
				t.Errorf("missing %s", name)
			}
		}
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"id":"grok_ok","object":"response","status":"completed"}`))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetGrokXAISubscription), transport)
	req := officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":false}`)
	req.Header.Set("Cookie", "inbound-secret")
	req.Header.Set("Proxy-Authorization", "inbound-proxy-secret")
	req.Header.Set("User-Agent", "inbound-user-agent")
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, req)
	if recorder.Code != http.StatusOK || !strings.Contains(recorder.Body.String(), "grok_ok") {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialGrokNonStreamFailureIsSanitized(t *testing.T) {
	transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"id":"failed","status":"failed","error":{"message":"private upstream detail"}}`))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetGrokXAISubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":false}`))
	if recorder.Code != http.StatusBadGateway || strings.Contains(recorder.Body.String(), "private upstream detail") {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialGrokRejectsTruncatedStream(t *testing.T) {
	transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
		stream := "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_grok\"}}\n\n"
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetGrokXAISubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":true}`))
	if recorder.Code != http.StatusOK || !strings.Contains(recorder.Body.String(), "response.created") || !strings.Contains(recorder.Body.String(), "upstream_error") {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialGrokAcceptsDataOnlyTerminalStreamAndCommentTrailer(t *testing.T) {
	transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
		stream := "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_grok\"}}\n\n" +
			"data: {\"type\":\"response.output_text.delta\",\"delta\":\"hello\"}\n\n" +
			"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_grok\",\"status\":\"completed\"}}\n\n" +
			": keep-alive\n\n"
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetGrokXAISubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":true}`))
	if recorder.Code != http.StatusOK || !strings.Contains(recorder.Body.String(), "response.completed") || strings.Contains(recorder.Body.String(), "upstream_error") {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialGrokContinuationStaysOnOriginalMember(t *testing.T) {
	config := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
	config.Edges[0].SchedulePolicy = policyRoundRobin
	first := config.Edges[0].Members[0]
	first.ID, first.SourceID, first.UpstreamKey = "account:first", "first", "access-first"
	second := first
	second.ID, second.SourceID, second.UpstreamKey = "account:second", "second", "access-second"
	config.Edges[0].Members = []RuntimeMemberConfig{first, second}
	var authorizations []string
	transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
		authorizations = append(authorizations, req.Header.Get("Authorization"))
		responseID := "resp_first"
		if len(authorizations) > 1 {
			responseID = "resp_second"
		}
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"id":"` + responseID + `","object":"response","status":"completed"}`))}, nil
	})
	rt := servingOfficialRuntime(t, config, transport)
	firstRecorder := httptest.NewRecorder()
	rt.handleResponses(firstRecorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"first","stream":false}`))
	secondRecorder := httptest.NewRecorder()
	rt.handleResponses(secondRecorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"second","stream":false,"previous_response_id":"resp_first"}`))
	if firstRecorder.Code != http.StatusOK || secondRecorder.Code != http.StatusOK {
		t.Fatalf("statuses=%d,%d bodies=%q,%q", firstRecorder.Code, secondRecorder.Code, firstRecorder.Body.String(), secondRecorder.Body.String())
	}
	if len(authorizations) != 2 || authorizations[0] != "Bearer access-first" || authorizations[1] != "Bearer access-first" {
		t.Fatalf("authorizations=%v", authorizations)
	}
}

func TestOfficialGrokUnknownContinuationFailsClosed(t *testing.T) {
	called := false
	transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
		called = true
		return nil, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetGrokXAISubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","previous_response_id":"unknown"}`))
	if recorder.Code != http.StatusBadRequest || called || !strings.Contains(recorder.Body.String(), "continuation_unavailable") {
		t.Fatalf("status=%d called=%v body=%q", recorder.Code, called, recorder.Body.String())
	}
}

func TestCappedCaptureStopsGrowingAtReplayLimit(t *testing.T) {
	capture := newCappedCapture(4)
	written, err := capture.Write([]byte("12345678"))
	if err != nil || written != 8 || string(capture.Bytes()) != "1234" || !capture.Overflowed() {
		t.Fatalf("written=%d err=%v bytes=%q overflow=%v", written, err, capture.Bytes(), capture.Overflowed())
	}
}

func TestOfficialResponsesErrorEventIsNotForwarded(t *testing.T) {
	transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
		stream := "event: error\ndata: {\"type\":\"error\",\"sequence_number\":0,\"message\":\"private upstream detail\"}\n\n"
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
	})
	rt := servingOfficialRuntime(t, officialRuntimeConfig(upstreamTargetCodexChatGPTSubscription), transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"probe","stream":true}`))
	if recorder.Code != http.StatusOK || strings.Contains(recorder.Body.String(), "private upstream detail") || !strings.Contains(recorder.Body.String(), `"type":"error"`) {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialLoginConfigBindsAccountMetadata(t *testing.T) {
	codex := officialRuntimeConfig(upstreamTargetCodexChatGPTSubscription)
	codex.Edges[0].Members[0].OfficialAccountID = ""
	if err := validateRuntimeConfig(codex); err == nil {
		t.Fatal("Codex official login accepted without account id")
	}
	grok := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
	grok.Edges[0].Members[0].OfficialAccountID = "must-not-be-present"
	if err := validateRuntimeConfig(grok); err == nil {
		t.Fatal("Grok official login accepted Codex account id")
	}
	crossDialect := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
	crossDialect.Edges[0].Dialect = "codex"
	if err := validateRuntimeConfig(crossDialect); err == nil {
		t.Fatal("Grok official login accepted a Codex downstream dialect")
	}
	crossDialect.Edges[0].GrokIngressCodexUpstream = true
	if err := validateRuntimeConfig(crossDialect); err == nil {
		t.Fatal("reverse pair flag opened Codex ingress to Grok upstream")
	}
	crossDialect.Edges[0].GrokIngressCodexUpstream = false
	crossDialect.Edges[0].CodexIngressGrokUpstream = true
	if err := validateRuntimeConfig(crossDialect); err != nil {
		t.Fatalf("matching pair flag did not open Codex ingress to Grok upstream: %v", err)
	}
}

func TestClassifyHTTPBodyKeepsOrdinaryForbiddenRequestScoped(t *testing.T) {
	if got := classifyHTTPBody(http.StatusForbidden, []byte(`{"error":"policy denied"}`)); got != classRequest {
		t.Fatalf("ordinary 403 class=%q", got)
	}
	if got := classifyHTTPBody(http.StatusForbidden, []byte(`{"code":"model_not_found"}`)); got != classEntitlement {
		t.Fatalf("model 403 class=%q", got)
	}
	if got := classifyHTTPBody(http.StatusNotFound, []byte(`{"error":"previous_response_id missing"}`)); got != classRequest {
		t.Fatalf("previous response 404 class=%q", got)
	}
}

func TestCodexIngressGrokUpstreamPairIsSanitizedEndToEnd(t *testing.T) {
	config := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
	config.Edges[0].Dialect = "codex"
	config.Edges[0].CodexIngressGrokUpstream = true
	transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
		raw, _ := io.ReadAll(req.Body)
		var body map[string]any
		_ = json.Unmarshal(raw, &body)
		if body["store"] != nil || body["metadata"] != nil || !strings.Contains(string(raw), "policy") {
			t.Fatalf("unsanitized Codex request reached Grok: %s", raw)
		}
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"id":"resp_pair","status":"completed","session_id":"private","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}],"x_grok_req_id":"private"}]}`))}, nil
	})
	rt := servingOfficialRuntime(t, config, transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","store":true,"metadata":{"private":true},"input":[{"role":"system","content":"policy"},{"role":"user","content":"ping"}]}`))
	if recorder.Code != http.StatusOK || strings.Contains(recorder.Body.String(), "private") || !strings.Contains(recorder.Body.String(), `"text":"ok"`) {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestGrokIngressCodexUpstreamPairIsSanitizedEndToEnd(t *testing.T) {
	config := officialRuntimeConfig(upstreamTargetCodexChatGPTSubscription)
	config.Edges[0].Dialect = "grok"
	config.Edges[0].GrokIngressCodexUpstream = true
	transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
		raw, _ := io.ReadAll(req.Body)
		var body map[string]any
		_ = json.Unmarshal(raw, &body)
		if body["prompt_cache_key"] != nil || body["reasoning"] != nil || body["stream"] != true || body["store"] != false {
			t.Fatalf("official Codex policy was not applied: %s", raw)
		}
		stream := "event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":0,\"response\":{\"id\":\"resp_pair\",\"status\":\"completed\",\"store\":false,\"metadata\":{\"private\":true},\"output\":[{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\",\"metadata\":{\"private\":true}}]}]}}\n\n"
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
	})
	rt := servingOfficialRuntime(t, config, transport)
	recorder := httptest.NewRecorder()
	rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","prompt_cache_key":"private","reasoning":{"effort":"high"},"input":"ping"}`))
	if recorder.Code != http.StatusOK || strings.Contains(recorder.Body.String(), "private") || strings.Contains(recorder.Body.String(), `"store"`) || !strings.Contains(recorder.Body.String(), `"text":"ok"`) {
		t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
	}
}

func TestOfficialPairStreamingSanitizesEverySSEEvent(t *testing.T) {
	t.Run("Codex to Grok", func(t *testing.T) {
		config := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
		config.Edges[0].Dialect = "codex"
		config.Edges[0].CodexIngressGrokUpstream = true
		transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
			stream := "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_pair\",\"session_id\":\"private\"}}\n\n" +
				"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_pair\",\"status\":\"completed\",\"x_grok_req_id\":\"private\"}}\n\n"
			return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
		})
		rt := servingOfficialRuntime(t, config, transport)
		recorder := httptest.NewRecorder()
		rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"ping","stream":true}`))
		if recorder.Code != http.StatusOK || strings.Contains(recorder.Body.String(), "private") || !strings.Contains(recorder.Body.String(), "response.completed") {
			t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
		}
		frames, err := splitCompleteResponsesSSEFrames(recorder.Body.Bytes())
		if err != nil {
			t.Fatalf("Codex pair stream framing: %v body=%q", err, recorder.Body.String())
		}
		state := newResponsesSSEState()
		for _, frame := range frames {
			if _, err := state.consumeFrame(frame); err != nil {
				t.Fatalf("Codex pair stream contract: %v frame=%q", err, frame)
			}
		}
		if err := state.finish(); err != nil {
			t.Fatalf("Codex pair stream terminal: %v body=%q", err, recorder.Body.String())
		}
	})

	t.Run("Grok to Codex", func(t *testing.T) {
		config := officialRuntimeConfig(upstreamTargetCodexChatGPTSubscription)
		config.Edges[0].Dialect = "grok"
		config.Edges[0].GrokIngressCodexUpstream = true
		transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
			stream := "event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0,\"response\":{\"id\":\"resp_pair\",\"metadata\":{\"private\":true}}}\n\n" +
				"event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":1,\"response\":{\"id\":\"resp_pair\",\"status\":\"completed\",\"store\":false,\"output\":[]}}\n\n"
			return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
		})
		rt := servingOfficialRuntime(t, config, transport)
		recorder := httptest.NewRecorder()
		rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"ping","stream":true}`))
		if recorder.Code != http.StatusOK || strings.Contains(recorder.Body.String(), "private") || strings.Contains(recorder.Body.String(), `"store"`) || !strings.Contains(recorder.Body.String(), "response.completed") {
			t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
		}
	})

	t.Run("Codex safe termination stays strict", func(t *testing.T) {
		config := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
		config.Edges[0].Dialect = "codex"
		config.Edges[0].CodexIngressGrokUpstream = true
		transport := roundTripFunc(func(*http.Request) (*http.Response, error) {
			stream := "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_pair\"}}\n\n"
			return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
		})
		rt := servingOfficialRuntime(t, config, transport)
		recorder := httptest.NewRecorder()
		rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"ping","stream":true}`))
		frames, err := splitCompleteResponsesSSEFrames(recorder.Body.Bytes())
		if err != nil {
			t.Fatalf("safe termination framing: %v body=%q", err, recorder.Body.String())
		}
		state := newResponsesSSEState()
		for _, frame := range frames {
			if _, err := state.consumeFrame(frame); err != nil {
				t.Fatalf("safe termination contract: %v frame=%q", err, frame)
			}
		}
		if err := state.finish(); err != nil || !strings.Contains(recorder.Body.String(), `"sequence_number":1`) {
			t.Fatalf("safe termination is not a strict terminal stream: %v body=%q", err, recorder.Body.String())
		}
	})
}

func TestOfficialPairsKeepResponseAndCacheAffinity(t *testing.T) {
	tests := []struct {
		name    string
		target  string
		dialect string
		flag    func(*RuntimeEdgeConfig)
	}{
		{
			name: "Codex to Grok", target: upstreamTargetGrokXAISubscription, dialect: "codex",
			flag: func(edge *RuntimeEdgeConfig) { edge.CodexIngressGrokUpstream = true },
		},
		{
			name: "Grok to Codex", target: upstreamTargetCodexChatGPTSubscription, dialect: "grok",
			flag: func(edge *RuntimeEdgeConfig) { edge.GrokIngressCodexUpstream = true },
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			config := officialRuntimeConfig(test.target)
			config.Edges[0].Dialect = test.dialect
			config.Edges[0].SchedulePolicy = policyRoundRobin
			test.flag(&config.Edges[0])
			first := config.Edges[0].Members[0]
			first.ID, first.SourceID, first.UpstreamKey = "account:first", "first", "access-first"
			second := first
			second.ID, second.SourceID, second.UpstreamKey = "account:second", "second", "access-second"
			config.Edges[0].Members = []RuntimeMemberConfig{first, second}

			var authorizations []string
			transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
				authorizations = append(authorizations, req.Header.Get("Authorization"))
				if test.target == upstreamTargetCodexChatGPTSubscription {
					stream := "event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":0,\"response\":{\"id\":\"resp_pair_affinity\",\"status\":\"completed\",\"output\":[]}}\n\n"
					return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
				}
				return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"id":"resp_pair_affinity","status":"completed","output":[]}`))}, nil
			})
			rt := servingOfficialRuntime(t, config, transport)
			for _, body := range []string{
				`{"model":"gpt-5-codex","input":"first","prompt_cache_key":"pair-cache"}`,
				`{"model":"gpt-5-codex","input":"same cache","prompt_cache_key":"pair-cache"}`,
				`{"model":"gpt-5-codex","input":"continue","previous_response_id":"resp_pair_affinity"}`,
				`{"model":"gpt-5-codex","input":"fallback","previous_response_id":"unknown","prompt_cache_key":"pair-cache"}`,
			} {
				recorder := httptest.NewRecorder()
				rt.handleResponses(recorder, officialResponsesRequest(body))
				if recorder.Code != http.StatusOK {
					t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
				}
			}
			if len(authorizations) != 4 {
				t.Fatalf("authorizations=%v", authorizations)
			}
			for _, got := range authorizations {
				if got != "Bearer access-first" {
					t.Fatalf("pair affinity drifted: %v", authorizations)
				}
			}
		})
	}
}

func TestCodexGrokPairLargeStreamStillKeepsAffinity(t *testing.T) {
	config := officialRuntimeConfig(upstreamTargetGrokXAISubscription)
	config.Edges[0].Dialect = "codex"
	config.Edges[0].CodexIngressGrokUpstream = true
	config.Edges[0].SchedulePolicy = policyRoundRobin
	first := config.Edges[0].Members[0]
	first.ID, first.SourceID, first.UpstreamKey = "account:first", "first", "access-first"
	second := first
	second.ID, second.SourceID, second.UpstreamKey = "account:second", "second", "access-second"
	config.Edges[0].Members = []RuntimeMemberConfig{first, second}

	largeDelta := strings.Repeat("x", grokOfficialReplayBodyBytes+1024)
	var authorizations []string
	transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
		authorizations = append(authorizations, req.Header.Get("Authorization"))
		if len(authorizations) == 1 {
			stream := "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_large_pair\"}}\n\n" +
				"data: {\"type\":\"response.output_text.delta\",\"delta\":\"" + largeDelta + "\"}\n\n" +
				"data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_large_pair\",\"status\":\"completed\"}}\n\n"
			return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
		}
		return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"id":"resp_large_pair_next","status":"completed","output":[]}`))}, nil
	})
	rt := servingOfficialRuntime(t, config, transport)
	firstRecorder := httptest.NewRecorder()
	rt.handleResponses(firstRecorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"first","stream":true,"prompt_cache_key":"large-pair"}`))
	secondRecorder := httptest.NewRecorder()
	rt.handleResponses(secondRecorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"continue","previous_response_id":"resp_large_pair"}`))
	if firstRecorder.Code != http.StatusOK || secondRecorder.Code != http.StatusOK {
		t.Fatalf("statuses=%d,%d", firstRecorder.Code, secondRecorder.Code)
	}
	if len(authorizations) != 2 || authorizations[0] != "Bearer access-first" || authorizations[1] != "Bearer access-first" {
		t.Fatalf("large stream affinity drifted: %v", authorizations)
	}
}

func TestOfficialPairCacheAffinityDoesNotRequireResponseID(t *testing.T) {
	for _, target := range []string{upstreamTargetGrokXAISubscription, upstreamTargetCodexChatGPTSubscription} {
		t.Run(target, func(t *testing.T) {
			config := officialRuntimeConfig(target)
			config.Edges[0].SchedulePolicy = policyRoundRobin
			if target == upstreamTargetGrokXAISubscription {
				config.Edges[0].Dialect = "codex"
				config.Edges[0].CodexIngressGrokUpstream = true
			} else {
				config.Edges[0].Dialect = "grok"
				config.Edges[0].GrokIngressCodexUpstream = true
			}
			first := config.Edges[0].Members[0]
			first.ID, first.SourceID, first.UpstreamKey = "account:first", "first", "access-first"
			second := first
			second.ID, second.SourceID, second.UpstreamKey = "account:second", "second", "access-second"
			config.Edges[0].Members = []RuntimeMemberConfig{first, second}
			var authorizations []string
			transport := roundTripFunc(func(req *http.Request) (*http.Response, error) {
				authorizations = append(authorizations, req.Header.Get("Authorization"))
				if target == upstreamTargetCodexChatGPTSubscription {
					stream := "event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":0,\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n"
					return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"text/event-stream"}}, Body: io.NopCloser(strings.NewReader(stream))}, nil
				}
				return &http.Response{StatusCode: http.StatusOK, Header: http.Header{"Content-Type": []string{"application/json"}}, Body: io.NopCloser(strings.NewReader(`{"status":"completed","output":[]}`))}, nil
			})
			rt := servingOfficialRuntime(t, config, transport)
			for _, input := range []string{"first", "same cache"} {
				recorder := httptest.NewRecorder()
				rt.handleResponses(recorder, officialResponsesRequest(`{"model":"gpt-5-codex","input":"`+input+`","prompt_cache_key":"seed-only"}`))
				if recorder.Code != http.StatusOK {
					t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
				}
			}
			if len(authorizations) != 2 || authorizations[0] != "Bearer access-first" || authorizations[1] != "Bearer access-first" {
				t.Fatalf("seed-only affinity drifted: %v", authorizations)
			}
		})
	}
}
