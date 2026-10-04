package main

import (
	"encoding/json"
	"net/http"
	"regexp"
	"strings"
	"testing"
)

func TestGrokOfficialSessionIdentityIsStableAndAccountIsolated(t *testing.T) {
	const seed = "codex:window:abc"
	if got := grokOfficialSessionID(seed, ""); got != "5ecc8048-570b-5d55-8418-1c6fb369a6d2" {
		t.Fatalf("single-account session=%q", got)
	}
	if got := grokOfficialSessionID(seed, "acc-a"); got != "c1c11348-2809-5acd-b3bf-289d0af3c1c6" {
		t.Fatalf("account A session=%q", got)
	}
	if got := grokOfficialSessionID(seed, "acc-b"); got != "b0e2d47d-4cc3-5526-8876-8ff5deab4200" {
		t.Fatalf("account B session=%q", got)
	}
	if got := grokOfficialSessionID("550E8400-E29B-41D4-A716-446655440000", ""); got != "550e8400-e29b-41d4-a716-446655440000" {
		t.Fatalf("canonical UUID=%q", got)
	}
	if first, second := stableGrokOfficialAgentID(), stableGrokOfficialAgentID(); first == "" || first != second {
		t.Fatalf("process agent id is not stable: %q %q", first, second)
	}
}

func TestPrepareGrokOfficialRequestUsesMetadataAndConfiguredModel(t *testing.T) {
	headers := make(http.Header)
	headers.Set("X-Codex-Turn-Metadata", `{"prompt_cache_key":"cache-from-header","window_id":"ignored"}`)
	prepared, err := prepareGrokOfficialRequest(
		[]byte(`{"model":"client-model","input":"hello","client_metadata":{"secret":"must-drop"}}`),
		headers,
		"account-1",
		"request-1",
		"grok-4.5",
	)
	if err != nil {
		t.Fatal(err)
	}
	if prepared.CacheSeed != "cache-from-header" || prepared.Body["prompt_cache_key"] != "cache-from-header" {
		t.Fatalf("cache seed was not injected: %+v", prepared)
	}
	if prepared.Body["model"] != "grok-4.5" || prepared.Identity.ModelOverride != "grok-4.5" {
		t.Fatalf("configured model was not applied: %+v", prepared)
	}
	if prepared.Identity.SessionID != grokOfficialSessionID("cache-from-header", "account-1") {
		t.Fatalf("session=%q", prepared.Identity.SessionID)
	}
	if _, exists := prepared.Body["client_metadata"]; exists {
		t.Fatal("client_metadata reached the prepared body")
	}
	if _, err := prepared.marshalBody(); err != nil {
		t.Fatal(err)
	}
}

func TestGrokOfficialHeadersAreAllowlistedAndDoNotRelayInboundSecrets(t *testing.T) {
	req, err := http.NewRequest(http.MethodPost, "https://cli-chat-proxy.grok.com/v1/responses", strings.NewReader(`{}`))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer inbound-secret")
	req.Header.Set("Cookie", "session=inbound-secret")
	req.Header.Set("Proxy-Authorization", "Basic inbound-secret")
	req.Header.Set("User-Agent", "inbound-user-agent")
	req.Header.Set("X-Grok-Agent-Id", "inbound-agent")
	identity := grokOfficialRequestIdentity{
		RequestID: "request-1", SessionID: "session-1", ModelOverride: "grok-4.5",
	}
	if err := applyGrokOfficialHeaders(req, "official-access-token", identity); err != nil {
		t.Fatal(err)
	}
	want := map[string]string{
		"Authorization":            "Bearer official-access-token",
		"X-Xai-Token-Auth":         "xai-grok-cli",
		"X-Grok-Client-Version":    "1.0.44",
		"X-Grok-Client-Identifier": "grok-shell",
		"X-Grok-Client-Mode":       "headless",
		"X-AuthenticateResponse":   "authenticate-response",
		"User-Agent":               "grok-pager/1.0.44 grok-shell/1.0.44",
		"X-Grok-Req-Id":            "request-1",
		"X-Grok-Session-Id":        "session-1",
		"X-Grok-Conv-Id":           "session-1",
		"X-Grok-Model-Override":    "grok-4.5",
	}
	for name, expected := range want {
		if got := req.Header.Get(name); got != expected {
			t.Errorf("%s=%q, want %q", name, got, expected)
		}
	}
	if req.Header.Get("Cookie") != "" || req.Header.Get("Proxy-Authorization") != "" {
		t.Fatal("inbound cookie or proxy authorization was retained")
	}
	if req.Header.Get("X-Grok-Agent-Id") != stableGrokOfficialAgentID() {
		t.Fatal("request did not use the process-stable agent id")
	}
	if !regexp.MustCompile(`^00-[0-9a-f]{32}-[0-9a-f]{16}-01$`).MatchString(req.Header.Get("Traceparent")) {
		t.Fatalf("traceparent=%q", req.Header.Get("Traceparent"))
	}
	if req.Header.Get("X-Grok-Turn-Idx") != "" {
		t.Fatal("unsupported turn index was invented")
	}
}

func TestGrokOfficialPromptCacheSeedPrecedenceAndTitleSuppression(t *testing.T) {
	body := map[string]any{
		"prompt_cache_key": "body-seed",
		"metadata":         map[string]any{"session_id": "metadata-seed"},
	}
	headers := make(http.Header)
	headers.Set("X-Codex-Turn-Metadata", `{"window_id":"window-1"}`)
	if got := extractGrokOfficialPromptCacheSeed(headers, body); got != "codex:window:window-1" {
		t.Fatalf("seed precedence=%q", got)
	}

	headers = make(http.Header)
	headers.Set("X-Claude-Code-Session-Id", "claude-session")
	title := map[string]any{"messages": []any{map[string]any{
		"role": "user", "content": "Generate a concise title for this coding session.",
	}}}
	if got := extractGrokOfficialPromptCacheSeed(headers, title); got != "" {
		t.Fatalf("title request received cache seed %q", got)
	}
	ordinary := map[string]any{"messages": []any{map[string]any{"role": "user", "content": "hello"}}}
	if got := extractGrokOfficialPromptCacheSeed(headers, ordinary); got != "claude:claude-session:agent:main" {
		t.Fatalf("Claude seed=%q", got)
	}
}

func TestPrepareGrokOfficialRequestNormalizesAndSanitizesTools(t *testing.T) {
	raw := []byte(`{
		"model":"grok-4.5",
		"input":[
			{"type":"message","role":"user","content":"hello"},
			{"type":"reasoning","encrypted_content":"enc","content":null}
		],
		"tools":[
			{"type":"local_shell"},
			{"type":"apply_patch"},
			{"type":"web_search","search_context_size":"high","external_web_access":true,"user_location":{"city":"x"}},
			{"type":"custom","name":"drop-me"}
		],
		"tool_choice":{"type":"function","name":"drop-me"},
		"reasoning":{"effort":"high","summary":"auto","generate_summary":"bad","unknown":"drop"},
		"store":false,
		"client_metadata":{"secret":"drop"}
	}`)
	prepared, err := prepareGrokOfficialRequest(raw, nil, "account-1", "request-1", "")
	if err != nil {
		t.Fatal(err)
	}
	tools := prepared.Body["tools"].([]any)
	if len(tools) != 3 {
		t.Fatalf("tools=%s", mustGrokOfficialJSON(t, tools))
	}
	shell := tools[0].(map[string]any)
	if shell["type"] != "shell" || shell["environment"].(map[string]any)["type"] != "local" {
		t.Fatalf("shell=%s", mustGrokOfficialJSON(t, shell))
	}
	patch := tools[1].(map[string]any)
	if patch["type"] != "function" || patch["name"] != "apply_patch" || patch["strict"] != true {
		t.Fatalf("apply_patch=%s", mustGrokOfficialJSON(t, patch))
	}
	web := tools[2].(map[string]any)
	for _, key := range []string{"search_context_size", "external_web_access", "user_location"} {
		if _, exists := web[key]; exists {
			t.Errorf("web_search retained %s", key)
		}
	}
	if _, exists := prepared.Body["tool_choice"]; exists {
		t.Fatal("tool_choice still names a removed tool")
	}
	reasoning := prepared.Body["reasoning"].(map[string]any)
	if len(reasoning) != 2 || reasoning["effort"] != "high" || reasoning["summary"] != "auto" {
		t.Fatalf("reasoning=%s", mustGrokOfficialJSON(t, reasoning))
	}
	input := prepared.Body["input"].([]any)
	if _, exists := input[1].(map[string]any)["content"]; exists {
		t.Fatal("invalid null reasoning content was retained")
	}
	if prepared.Body["store"] != false {
		t.Fatal("valid boolean store was removed")
	}
}

func TestGrokOfficialToolSanitizerDropsDependentFieldsWhenNoToolsRemain(t *testing.T) {
	body := map[string]any{
		"tools":               []any{map[string]any{"type": "unsupported"}},
		"tool_choice":         "required",
		"parallel_tool_calls": true,
		"store":               "false",
		"reasoning":           "high",
	}
	sanitizeGrokOfficialRequest(body)
	for _, key := range []string{"tools", "tool_choice", "parallel_tool_calls", "store", "reasoning"} {
		if _, exists := body[key]; exists {
			t.Errorf("%s was retained", key)
		}
	}
}

func TestGrokOfficialReasoningReplayIsIsolatedBoundedAndSkipsStatefulRequests(t *testing.T) {
	replay := newGrokOfficialReasoningReplay()
	completed := map[string]any{"output": []any{
		map[string]any{"type": "reasoning", "encrypted_content": "enc-a"},
		map[string]any{"type": "message", "role": "assistant"},
	}}
	replay.storeCompleted("account-a", "grok-4.5", "session-1", completed)

	body := map[string]any{"input": "hello"}
	replay.apply(body, "account-a", "grok-4.5", "session-1")
	input := body["input"].([]any)
	if len(input) != 2 || !isGrokOfficialEncryptedReasoning(input[0]) {
		t.Fatalf("replayed input=%s", mustGrokOfficialJSON(t, input))
	}
	isolated := map[string]any{"input": "hello"}
	replay.apply(isolated, "account-b", "grok-4.5", "session-1")
	if _, ok := isolated["input"].(string); !ok {
		t.Fatal("reasoning crossed account boundary")
	}
	stateful := map[string]any{"input": "hello", "previous_response_id": "response-1"}
	replay.apply(stateful, "account-a", "grok-4.5", "session-1")
	if _, ok := stateful["input"].(string); !ok {
		t.Fatal("reasoning was injected beside previous_response_id")
	}

	for index := 0; index < grokOfficialReplayEntries+1; index++ {
		replay.storeCompleted("bounded", "grok-4.5", "session-"+itoa(index), completed)
	}
	first := map[string]any{"input": "hello"}
	replay.apply(first, "bounded", "grok-4.5", "session-0")
	if _, ok := first["input"].(string); !ok {
		t.Fatal("oldest replay entry was not evicted")
	}
	last := map[string]any{"input": "hello"}
	replay.apply(last, "bounded", "grok-4.5", "session-64")
	if _, ok := last["input"].([]any); !ok {
		t.Fatal("newest replay entry was not retained")
	}
}

func TestGrokOfficialReasoningReplayCapturesSSEAndRecoversOnlyOnce(t *testing.T) {
	replay := newGrokOfficialReasoningReplay()
	sse := []byte("event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"model\":\"grok-4.5\",\"output\":[{\"type\":\"reasoning\",\"encrypted_content\":\"enc-sse\"}]}}\n\n")
	replay.storeSSE("account-a", "grok-4.5", "session-1", sse)
	body := map[string]any{"input": []any{map[string]any{"type": "message", "role": "user"}}}
	replay.apply(body, "account-a", "grok-4.5", "session-1")
	if !isGrokOfficialEncryptedReasoning(body["input"].([]any)[0]) {
		t.Fatal("SSE completed reasoning was not captured")
	}
	failure := []byte(`{"error":{"message":"could not decrypt the provided encrypted_content","secret":"not-for-errors"}}`)
	if !replay.recoverDecodeFailure(body, "account-a", "grok-4.5", "session-1", failure, false) {
		t.Fatal("decode failure did not strip encrypted reasoning")
	}
	if replay.recoverDecodeFailure(body, "account-a", "grok-4.5", "session-1", failure, true) {
		t.Fatal("decode failure helper allowed a second retry")
	}
	again := map[string]any{"input": "hello"}
	replay.apply(again, "account-a", "grok-4.5", "session-1")
	if _, ok := again["input"].(string); !ok {
		t.Fatal("decode recovery did not clear replay cache")
	}
	if !isGrokOfficialReasoningDecodeFailure([]byte("Could not decode the compaction blob")) {
		t.Fatal("compaction marker was not recognized")
	}
	if isGrokOfficialReasoningDecodeFailure([]byte("ordinary bad request")) {
		t.Fatal("ordinary request error was marked recoverable")
	}
}

func TestGrokOfficialErrorsDoNotEchoBodyOrSecret(t *testing.T) {
	secret := "super-secret-token"
	_, err := prepareGrokOfficialRequest([]byte(`{"secret":"`+secret+`"} trailing`), nil, "account", "request", "")
	if err == nil || strings.Contains(err.Error(), secret) || strings.Contains(err.Error(), "trailing`") {
		t.Fatalf("unsafe parse error=%v", err)
	}
	req, requestErr := http.NewRequest(http.MethodPost, "https://cli-chat-proxy.grok.com/v1/responses", nil)
	if requestErr != nil {
		t.Fatal(requestErr)
	}
	err = applyGrokOfficialHeaders(req, secret+"\r\nInjected: true", grokOfficialRequestIdentity{RequestID: "request"})
	if err == nil || strings.Contains(err.Error(), secret) {
		t.Fatalf("unsafe header error=%v", err)
	}
}

func mustGrokOfficialJSON(t *testing.T, value any) string {
	t.Helper()
	raw, err := json.Marshal(value)
	if err != nil {
		t.Fatal(err)
	}
	return string(raw)
}
