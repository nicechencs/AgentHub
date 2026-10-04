package main

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestEncodeResponsesToChatCompleteSubset(t *testing.T) {
	body := []byte(`{
  "model":"kimi-k2.5","instructions":"be concise","stream":true,
  "input":[
    {"type":"message","role":"developer","content":[{"type":"input_text","text":"policy"}]},
    {"type":"function_call","call_id":"call_a","name":"weather","arguments":"{\"city\":\"Shenzhen\"}"},
    {"type":"function_call","call_id":"call_b","name":"clock","arguments":"{}"},
    {"type":"function_call_output","call_id":"call_a","output":[{"type":"input_text","text":"sunny"}]}
  ],
  "tools":[{"type":"function","name":"weather","description":"forecast","parameters":{"type":"object"},"strict":true}],
  "tool_choice":{"type":"function","name":"weather"},
  "top_p":0.8,"max_output_tokens":64,"temperature":1,"metadata":{"secret":"drop"}
}`)
	encoded, stream, err := encodeResponsesToChat(body)
	if err != nil {
		t.Fatal(err)
	}
	if !stream {
		t.Fatal("stream=false")
	}
	var got map[string]any
	if err := json.Unmarshal(encoded, &got); err != nil {
		t.Fatal(err)
	}
	if got["temperature"] != nil || got["metadata"] != nil {
		t.Fatalf("unsafe passthrough: %s", encoded)
	}
	if got["max_tokens"].(float64) != 64 || got["top_p"].(float64) != 0.8 {
		t.Fatalf("mapped options: %s", encoded)
	}
	if got["stream_options"].(map[string]any)["include_usage"] != true {
		t.Fatalf("missing stream usage: %s", encoded)
	}
	messages := got["messages"].([]any)
	if len(messages) != 4 {
		t.Fatalf("messages=%#v", messages)
	}
	if messages[0].(map[string]any)["role"] != "system" || messages[1].(map[string]any)["role"] != "system" {
		t.Fatalf("instructions/developer mapping: %#v", messages)
	}
	assistant := messages[2].(map[string]any)
	if len(assistant["tool_calls"].([]any)) != 2 || assistant["content"] != nil {
		t.Fatalf("parallel calls not grouped: %#v", assistant)
	}
	toolResult := messages[3].(map[string]any)
	if toolResult["role"] != "tool" || toolResult["tool_call_id"] != "call_a" || toolResult["content"] != "sunny" {
		t.Fatalf("tool result: %#v", toolResult)
	}
	tools := got["tools"].([]any)
	function := tools[0].(map[string]any)["function"].(map[string]any)
	if function["name"] != "weather" || function["strict"] != true {
		t.Fatalf("tools: %#v", tools)
	}
}

func TestEncodeResponsesToChatRejectsLossyInputs(t *testing.T) {
	tests := []string{
		`{"model":"m","input":[{"role":"user","content":[{"type":"input_image","image_url":"x"}]}]}`,
		`{"model":"m","input":[{"type":"item_reference","id":"x"}]}`,
		`{"model":"m","input":[{"type":"function_call","call_id":"c","name":"f","arguments":{}}]}`,
	}
	for _, body := range tests {
		if _, _, err := encodeResponsesToChat([]byte(body)); err == nil {
			t.Fatalf("accepted unsupported request: %s", body)
		}
	}
}

func TestEncodeResponsesToChatDropsHostedToolsWithoutDroppingFunctions(t *testing.T) {
	encoded, _, err := encodeResponsesToChat([]byte(`{
  "model":"m","input":"x",
  "tools":[{"type":"web_search_preview"},{"type":"function","name":"f"}],
  "tool_choice":{"type":"web_search_preview"}
}`))
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(encoded, &got); err != nil {
		t.Fatal(err)
	}
	tools := got["tools"].([]any)
	if len(tools) != 1 || tools[0].(map[string]any)["type"] != "function" {
		t.Fatalf("tools=%#v", tools)
	}
	if _, exists := got["tool_choice"]; exists {
		t.Fatalf("hosted tool choice was forwarded: %s", encoded)
	}
}

func TestEncodeResponsesToChatAcceptsOptionalNullsAndEmptyMessage(t *testing.T) {
	encoded, _, err := encodeResponsesToChat([]byte(`{
  "model":"m","instructions":null,
  "input":[{"type":"message","role":"assistant","name":null},
    {"type":"message","role":"user","content":[{"type":"refusal","text":"no"}]}],
  "tools":[{"type":"function","name":"f","description":null}]
}`))
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(encoded, &got); err != nil {
		t.Fatal(err)
	}
	if _, exists := got["instructions"]; exists {
		t.Fatalf("instructions leaked into Chat request: %s", encoded)
	}
	messages := got["messages"].([]any)
	if messages[0].(map[string]any)["content"] != "" || messages[1].(map[string]any)["content"] != "no" {
		t.Fatalf("optional message fields: %#v", messages)
	}
}

func TestTranslateChatJSONToResponsesToolsUsageAndIncomplete(t *testing.T) {
	body := []byte(`{
  "id":"chatcmpl_1","model":"kimi-k2.5","created":42,
  "choices":[{"finish_reason":"length","message":{"content":"partial","tool_calls":[
    {"id":"call_7","type":"function","function":{"name":"weather","arguments":"{\"city\":\"SZ\"}"}}
  ]}}],
  "usage":{"prompt_tokens":7,"completion_tokens":3,"total_tokens":10,
    "prompt_tokens_details":{"cached_tokens":2},"completion_tokens_details":{"reasoning_tokens":1}}
}`)
	encoded, err := translateChatJSONToResponses(body)
	if err != nil {
		t.Fatal(err)
	}
	var got map[string]any
	if err := json.Unmarshal(encoded, &got); err != nil {
		t.Fatal(err)
	}
	if got["object"] != "response" || got["status"] != "incomplete" {
		t.Fatalf("response: %s", encoded)
	}
	if got["incomplete_details"].(map[string]any)["reason"] != "max_output_tokens" {
		t.Fatalf("incomplete: %s", encoded)
	}
	output := got["output"].([]any)
	if len(output) != 2 || output[1].(map[string]any)["call_id"] != "call_7" {
		t.Fatalf("output: %#v", output)
	}
	usage := got["usage"].(map[string]any)
	if usage["input_tokens"].(float64) != 7 || usage["reasoning_tokens"].(float64) != 1 {
		t.Fatalf("usage: %#v", usage)
	}
}

func TestTranslateChatJSONToResponsesLegacyToolAndSafeError(t *testing.T) {
	encoded, err := translateChatJSONToResponses([]byte(`{"choices":[{"finish_reason":"tool_calls","message":{"function_call":{"name":"legacy","arguments":"{}"}}}]}`))
	if err != nil || !strings.Contains(string(encoded), `"call_id":"call_0"`) {
		t.Fatalf("legacy: %s err=%v", encoded, err)
	}
	_, err = translateChatJSONToResponses([]byte(`{"error":{"message":"upstream body secret"}}`))
	if err == nil || strings.Contains(err.Error(), "secret") {
		t.Fatalf("unsafe upstream error: %v", err)
	}
}

func TestChatToResponsesSSETextToolUsageAndIncomplete(t *testing.T) {
	codec := newChatToResponsesSSE("requested-model")
	chunks := []string{
		`{"id":"chat","model":"kimi-k2.5","created":7,"choices":[{"delta":{"content":"hi "}}]}`,
		`{"choices":[{"delta":{"content":"there","tool_calls":[{"index":0,"id":"call_9","function":{"name":"weather","arguments":"{\"ci"}}]}}]}`,
		`{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"ty\":\"SZ\"}"}}]},"finish_reason":"content_filter"}]}`,
		`{"choices":[],"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}`,
	}
	var wire strings.Builder
	for _, chunk := range chunks {
		frames, done, err := codec.consume([]byte(chunk))
		if err != nil || done {
			t.Fatalf("consume done=%v err=%v", done, err)
		}
		wire.Write(frames)
	}
	frames, done, err := codec.consume([]byte("[DONE]"))
	if err != nil || !done {
		t.Fatalf("finish done=%v err=%v", done, err)
	}
	wire.Write(frames)
	got := wire.String()
	for _, event := range []string{
		"response.created", "response.in_progress", "response.output_text.delta",
		"response.function_call_arguments.delta", "response.function_call_arguments.done",
		"response.output_item.done", "response.incomplete",
	} {
		if !strings.Contains(got, "event: "+event+"\n") {
			t.Fatalf("missing %s in %s", event, got)
		}
	}
	if !strings.Contains(got, `"arguments":"{\"city\":\"SZ\"}"`) || !strings.Contains(got, `"input_tokens":5`) {
		t.Fatalf("tool/usage state lost: %s", got)
	}
	if extra := codec.finish(); len(extra) != 0 {
		t.Fatalf("finish not idempotent: %s", extra)
	}
}

func TestChatToResponsesSSERejectsMalformedAndUpstreamError(t *testing.T) {
	codec := newChatToResponsesSSE("m")
	if _, _, err := codec.consume([]byte(`{"error":{"message":"do not echo me"}}`)); err == nil || strings.Contains(err.Error(), "echo") {
		t.Fatalf("unsafe error: %v", err)
	}
	codec = newChatToResponsesSSE("m")
	if _, _, err := codec.consume([]byte(`not-json`)); err == nil {
		t.Fatal("malformed chunk accepted")
	}
}

func TestChatToResponsesSSEFailureIsSafeAndTerminal(t *testing.T) {
	codec := newChatToResponsesSSE("m")
	frames, _, err := codec.consume([]byte(`{"choices":[{"delta":{"content":"partial"}}]}`))
	if err != nil || len(frames) == 0 {
		t.Fatalf("initial frames=%s err=%v", frames, err)
	}
	failure := string(codec.fail())
	if !strings.Contains(failure, "event: error\n") || !strings.Contains(failure, `"code":"upstream_error"`) || !strings.Contains(failure, `"param":null`) {
		t.Fatalf("failure=%s", failure)
	}
	if strings.Contains(failure, "partial") {
		t.Fatalf("failure echoed content: %s", failure)
	}
	if extra := codec.finish(); len(extra) != 0 {
		t.Fatalf("failed stream later completed: %s", extra)
	}
}
