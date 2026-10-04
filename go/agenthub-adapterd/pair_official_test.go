package main

import (
	"encoding/json"
	"strings"
	"testing"
)

func decodeOfficialPairTestBody(t *testing.T, raw []byte) map[string]any {
	t.Helper()
	var body map[string]any
	if err := json.Unmarshal(raw, &body); err != nil {
		t.Fatalf("decode pair body: %v", err)
	}
	return body
}

func TestPrepareCodexIngressGrokRequestFoldsAndCleans(t *testing.T) {
	raw := []byte(`{
  "model":"grok-4.5","stream":false,"store":true,
  "metadata":{"secret":true},"service_tier":"default","max_tokens":7,
  "instructions":"Be brief.",
  "input":[
    {"type":"message","role":"system","content":[{"type":"input_text","text":"policy"}]},
    {"type":"message","role":"user","content":[{"type":"input_text","text":"hello"}]}
  ],
  "reasoning":{"effort":"high"},"prompt_cache_key":"cache",
  "include":["reasoning.encrypted_content"],"temperature":0.2,
  "tools":[{"type":"local_shell"},{"type":"function","name":"lookup"}]
}`)
	prepared, downstreamStream, err := prepareCodexIngressGrokRequest(raw)
	if err != nil {
		t.Fatal(err)
	}
	if downstreamStream {
		t.Fatal("downstream stream preference changed")
	}
	body := decodeOfficialPairTestBody(t, prepared)
	for _, key := range []string{"store", "metadata", "service_tier", "max_tokens"} {
		if _, exists := body[key]; exists {
			t.Errorf("Codex-only key %q survived: %s", key, prepared)
		}
	}
	if body["instructions"] != "Be brief.\npolicy" {
		t.Fatalf("instructions=%v", body["instructions"])
	}
	if len(body["input"].([]any)) != 1 || body["reasoning"] == nil || body["prompt_cache_key"] != "cache" {
		t.Fatalf("Grok-capable fields were lost: %s", prepared)
	}
}

func TestPrepareGrokIngressCodexRequestReusesOfficialPolicy(t *testing.T) {
	raw := []byte(`{
  "model":"grok-4.5","stream":false,"store":true,
  "prompt_cache_key":"cache","previous_response_id":"resp_old",
  "reasoning":{"effort":"high"},"include":["reasoning.encrypted_content"],
  "metadata":{"grok":true},"max_output_tokens":128,
  "input":[{"role":"system","content":"policy"},{"role":"user","content":"ping"}],
  "tools":[{"type":"function","name":"lookup"}]
}`)
	prepared, downstreamStream, err := prepareGrokIngressCodexRequest(raw, "gpt-5.6-sol")
	if err != nil {
		t.Fatal(err)
	}
	if downstreamStream {
		t.Fatal("downstream stream preference changed")
	}
	body := decodeOfficialPairTestBody(t, prepared)
	if body["model"] != "gpt-5.6-sol" || body["store"] != false || body["stream"] != true {
		t.Fatalf("official Codex policy not applied: %s", prepared)
	}
	for _, key := range []string{"prompt_cache_key", "previous_response_id", "reasoning", "include", "metadata", "max_output_tokens"} {
		if _, exists := body[key]; exists {
			t.Errorf("non-allowlisted key %q survived: %s", key, prepared)
		}
	}
	if body["input"].([]any)[0].(map[string]any)["role"] != "user" {
		t.Fatalf("system item survived: %s", prepared)
	}
}

func TestSanitizeGrokOfficialResponseForCodexRecursively(t *testing.T) {
	raw := []byte(`{
  "id":"resp_grok","prompt_cache_key":"secret","session_id":"secret",
  "output":[
    {"type":"reasoning","encrypted_content":"enc","nested":{"x-grok-debug":"drop","keep":1}},
    {"type":"message","content":[{"type":"output_text","text":"hello"}]}
  ],
  "usage":{"total_tokens":10,"reasoning_tokens":2},
  "x_grok_req_id":"secret","conversation_id":"secret"
}`)
	sanitized, err := sanitizeGrokOfficialResponseForCodex(raw)
	if err != nil {
		t.Fatal(err)
	}
	body := decodeOfficialPairTestBody(t, sanitized)
	for _, key := range []string{"prompt_cache_key", "session_id", "x_grok_req_id", "conversation_id"} {
		if _, exists := body[key]; exists {
			t.Errorf("Grok identity key %q survived: %s", key, sanitized)
		}
	}
	nested := body["output"].([]any)[0].(map[string]any)["nested"].(map[string]any)
	if _, exists := nested["x-grok-debug"]; exists || nested["keep"] != float64(1) {
		t.Fatalf("nested sanitization failed: %s", sanitized)
	}
	if body["output"].([]any)[0].(map[string]any)["encrypted_content"] != "enc" || body["usage"].(map[string]any)["total_tokens"] != float64(10) {
		t.Fatalf("standard response data lost: %s", sanitized)
	}
}

func TestSanitizeCodexOfficialResponseAndSSEForGrok(t *testing.T) {
	response, err := sanitizeCodexOfficialResponseForGrok([]byte(`{
  "id":"resp_codex","store":false,"service_tier":"default","metadata":{"secret":true},
  "output":[{"type":"function_call","call_id":"call_lookup","name":"lookup","arguments":"{}","metadata":{"drop":true}}],
  "usage":{"total_tokens":12}
}`))
	if err != nil {
		t.Fatal(err)
	}
	body := decodeOfficialPairTestBody(t, response)
	if _, exists := body["store"]; exists {
		t.Fatalf("store survived: %s", response)
	}
	tool := body["output"].([]any)[0].(map[string]any)
	if _, exists := tool["metadata"]; exists || tool["call_id"] != "call_lookup" || tool["arguments"] != "{}" {
		t.Fatalf("tool event damaged: %s", response)
	}

	event, err := sanitizeCodexOfficialSSEEventForGrok([]byte(`{
  "type":"error","sequence_number":3,"code":"upstream_error",
  "message":"provider failed","store":false,"metadata":{"codex":true}
}`))
	if err != nil {
		t.Fatal(err)
	}
	eventBody := decodeOfficialPairTestBody(t, event)
	if eventBody["type"] != "error" || eventBody["code"] != "upstream_error" {
		t.Fatalf("error event damaged: %s", event)
	}
	if _, exists := eventBody["metadata"]; exists {
		t.Fatalf("Codex metadata leaked: %s", event)
	}
}

func TestSanitizeGrokOfficialSSEEventForCodexKeepsDelta(t *testing.T) {
	event, err := sanitizeGrokOfficialSSEEventForCodex([]byte(`{
  "type":"response.output_text.delta","sequence_number":1,"delta":"hi",
  "response":{"id":"resp","session_id":"secret","prompt_cache_key":"secret"}
}`))
	if err != nil {
		t.Fatal(err)
	}
	body := decodeOfficialPairTestBody(t, event)
	if body["delta"] != "hi" {
		t.Fatalf("delta lost: %s", event)
	}
	response := body["response"].(map[string]any)
	if _, exists := response["session_id"]; exists {
		t.Fatalf("nested session leaked: %s", event)
	}
}

func TestOfficialPairHelpersRejectInvalidWithoutEcho(t *testing.T) {
	secret := "pair-secret-must-not-escape"
	for _, call := range []func() error{
		func() error { _, _, err := prepareCodexIngressGrokRequest([]byte(`[` + secret + `]`)); return err },
		func() error { _, _, err := prepareGrokIngressCodexRequest([]byte(`null`), ""); return err },
		func() error {
			_, err := sanitizeGrokOfficialResponseForCodex([]byte(`{"a":1} trailing ` + secret))
			return err
		},
		func() error { _, err := sanitizeCodexOfficialSSEEventForGrok([]byte(`[]`)); return err },
	} {
		err := call()
		if err == nil || strings.Contains(err.Error(), secret) {
			t.Fatalf("unsafe error: %v", err)
		}
	}
}
