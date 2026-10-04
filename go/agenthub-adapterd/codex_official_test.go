package main

import (
	"encoding/json"
	"strings"
	"testing"
)

func decodePreparedCodexBody(t *testing.T, body []byte) map[string]any {
	t.Helper()
	var decoded map[string]any
	if err := json.Unmarshal(body, &decoded); err != nil {
		t.Fatalf("decode prepared body: %v", err)
	}
	return decoded
}

func TestPrepareOfficialCodexRequestForcesAndAllowlists(t *testing.T) {
	prepared, downstreamStream, err := prepareOfficialCodexRequest([]byte(`{
  "model":"gpt-5.6-sol","stream":false,"store":true,
  "input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"ping"}]}],
  "instructions":"brief","tools":[{"type":"function","name":"echo"}],"tool_choice":"auto","top_p":0.9,
  "metadata":{"secret":"must-drop"},"temperature":0.2,"max_output_tokens":42,"user":"alice"
}`), "")
	if err != nil {
		t.Fatal(err)
	}
	if downstreamStream {
		t.Fatal("downstream stream preference changed")
	}
	body := decodePreparedCodexBody(t, prepared)
	if body["stream"] != true || body["store"] != false || body["model"] != "gpt-5.6-sol" {
		t.Fatalf("forced fields: %s", prepared)
	}
	for _, key := range []string{"metadata", "temperature", "max_output_tokens", "user"} {
		if _, exists := body[key]; exists {
			t.Errorf("disallowed key %q survived: %s", key, prepared)
		}
	}
}

func TestPrepareOfficialCodexRequestFoldsSystemAndDeveloper(t *testing.T) {
	prepared, downstreamStream, err := prepareOfficialCodexRequest([]byte(`{
  "stream":true,"instructions":"existing",
  "input":[
    {"type":"message","role":"system","content":[{"type":"input_text","text":"system"}]},
    {"type":"message","role":"user","content":[{"type":"input_text","text":"ping"}]},
    {"type":"message","role":"developer","content":"developer"}
  ]
}`), "gpt-5.6-sol")
	if err != nil {
		t.Fatal(err)
	}
	if !downstreamStream {
		t.Fatal("downstream stream=true was lost")
	}
	body := decodePreparedCodexBody(t, prepared)
	if body["instructions"] != "existing\nsystem\ndeveloper" || body["model"] != "gpt-5.6-sol" {
		t.Fatalf("folded body: %s", prepared)
	}
	input := body["input"].([]any)
	if len(input) != 1 || input[0].(map[string]any)["role"] != "user" {
		t.Fatalf("system/developer items survived: %s", prepared)
	}
}

func TestPrepareOfficialCodexRequestPrependsFoldedTextWithoutInstructions(t *testing.T) {
	prepared, _, err := prepareOfficialCodexRequest([]byte(`{
  "model":"claude-sonnet-4","input":[
    {"role":"system","content":"policy"},
    {"role":"developer","content":[{"text":"guard"}]},
    {"role":"user","content":[{"type":"input_text","text":"ping"}]}
  ]
}`), "")
	if err != nil {
		t.Fatal(err)
	}
	body := decodePreparedCodexBody(t, prepared)
	if _, exists := body["model"]; exists {
		t.Fatalf("leftover model survived: %s", prepared)
	}
	if _, exists := body["instructions"]; exists {
		t.Fatalf("instructions should stay absent: %s", prepared)
	}
	text := body["input"].([]any)[0].(map[string]any)["content"].([]any)[0].(map[string]any)["text"]
	if text != "policy\nguard\nping" {
		t.Fatalf("prepended text=%q", text)
	}
}

func TestPrepareOfficialCodexRequestModelPolicy(t *testing.T) {
	tests := []struct {
		name       string
		incoming   string
		configured string
		want       string
		wantModel  bool
	}{
		{"incoming official", " o3 ", "", "o3", true},
		{"configured wins", "o3", " gpt-5.6-sol ", "gpt-5.6-sol", true},
		{"configured leftover ignored", "gpt-5.6", "grok-4.6", "gpt-5.6", true},
		{"incoming leftover removed", "agenthub_codex_bridge", "", "", false},
		{"non-string omitted", "", "", "", false},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			raw := `{"input":[],"model":` + strconvQuote(test.incoming) + `}`
			if test.name == "non-string omitted" {
				raw = `{"input":[],"model":42}`
			}
			prepared, _, err := prepareOfficialCodexRequest([]byte(raw), test.configured)
			if err != nil {
				t.Fatal(err)
			}
			body := decodePreparedCodexBody(t, prepared)
			got, exists := body["model"]
			if exists != test.wantModel || (exists && got != test.want) {
				t.Fatalf("model=%v exists=%v body=%s", got, exists, prepared)
			}
		})
	}
}

func TestPrepareOfficialCodexRequestRejectsInvalidWithoutEcho(t *testing.T) {
	secret := "request-secret-must-not-escape"
	for _, raw := range []string{`[` + secret + `]`, `{"input":[]} trailing`, `null`} {
		_, _, err := prepareOfficialCodexRequest([]byte(raw), "")
		if err == nil || strings.Contains(err.Error(), secret) {
			t.Fatalf("unsafe error for %q: %v", raw, err)
		}
	}
}

func strconvQuote(value string) string {
	encoded, _ := json.Marshal(value)
	return string(encoded)
}
