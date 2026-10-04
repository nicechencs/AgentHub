package main

import (
	"encoding/json"
	"strconv"
	"strings"
	"testing"
)

func responsesFrame(kind string, sequence uint64, fields string) []byte {
	if fields != "" {
		fields = "," + fields
	}
	return []byte("event: " + kind + "\ndata: {\"type\":\"" + kind + "\",\"sequence_number\":" + strconv.FormatUint(sequence, 10) + fields + "}\n\n")
}

func TestResponsesSSEStateValidatesSequenceAndTerminal(t *testing.T) {
	state := newResponsesSSEState()
	for _, frame := range [][]byte{
		responsesFrame("response.created", 0, `"response":{"id":"resp_1"}`),
		responsesFrame("response.output_text.delta", 1, `"delta":"ok"`),
		responsesFrame("response.completed", 2, `"response":{"id":"resp_1","output":[]}`),
	} {
		if _, err := state.consumeFrame(frame); err != nil {
			t.Fatal(err)
		}
	}
	if err := state.finish(); err != nil {
		t.Fatal(err)
	}
	if _, err := state.consumeFrame(responsesFrame("response.incomplete", 3, `"response":{"output":[]}`)); err == nil {
		t.Fatal("accepted data after terminal event")
	}
}

func TestResponsesSSEStateRejectsMalformedFrames(t *testing.T) {
	tests := map[string][]byte{
		"done marker":       []byte("event: response.completed\ndata: [DONE]\n\n"),
		"missing event":     []byte("data: {\"type\":\"response.created\",\"sequence_number\":0}\n\n"),
		"duplicate event":   []byte("event: response.created\nevent: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0}\n\n"),
		"event mismatch":    []byte("event: response.created\ndata: {\"type\":\"response.in_progress\",\"sequence_number\":0}\n\n"),
		"unknown event":     []byte("event: response.future\ndata: {\"type\":\"response.future\",\"sequence_number\":0}\n\n"),
		"missing sequence":  []byte("event: response.created\ndata: {\"type\":\"response.created\"}\n\n"),
		"fraction sequence": []byte("event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0.5}\n\n"),
		"max sequence":      responsesFrame("response.created", ^uint64(0), `"response":{}`),
		"multiple json":     []byte("event: response.created\ndata: {\"type\":\"response.created\",\"sequence_number\":0} {}\n\n"),
	}
	for name, frame := range tests {
		t.Run(name, func(t *testing.T) {
			if _, err := newResponsesSSEState().consumeFrame(frame); err == nil {
				t.Fatal("accepted malformed frame")
			}
		})
	}

	state := newResponsesSSEState()
	if _, err := state.consumeFrame(responsesFrame("response.created", 2, `"response":{}`)); err != nil {
		t.Fatal(err)
	}
	if _, err := state.consumeFrame(responsesFrame("response.in_progress", 2, `"response":{}`)); err == nil {
		t.Fatal("accepted repeated sequence number")
	}
}

func TestAggregateOfficialResponsesSSERebuildsTextAndPreservesTerminalMetadata(t *testing.T) {
	body := append([]byte{}, responsesFrame("response.created", 0, `"response":{"id":"resp_text","model":"gpt-5.6-sol"}`)...)
	body = append(body, responsesFrame("response.output_text.delta", 1, `"delta":"hello "`)...)
	body = append(body, responsesFrame("response.output_text.delta", 2, `"delta":"world"`)...)
	body = append(body, responsesFrame("response.completed", 3, `"response":{"id":"resp_text","model":"gpt-5.6-sol","status":"completed","output":[],"usage":{"input_tokens":3,"output_tokens":2}}`)...)

	aggregated, err := aggregateOfficialResponsesSSE(body, int64(len(body)))
	if err != nil {
		t.Fatal(err)
	}
	var response map[string]any
	if err := json.Unmarshal(aggregated, &response); err != nil {
		t.Fatal(err)
	}
	if response["id"] != "resp_text" || response["model"] != "gpt-5.6-sol" {
		t.Fatalf("metadata not preserved: %s", aggregated)
	}
	if response["usage"].(map[string]any)["input_tokens"] != float64(3) {
		t.Fatalf("usage not preserved: %s", aggregated)
	}
	text := response["output"].([]any)[0].(map[string]any)["content"].([]any)[0].(map[string]any)["text"]
	if text != "hello world" {
		t.Fatalf("text=%q response=%s", text, aggregated)
	}
}

func TestAggregateOfficialResponsesSSERebuildsFunctionCall(t *testing.T) {
	body := append([]byte{}, responsesFrame("response.created", 0, `"response":{"id":"resp_tool","model":"gpt-5.6-sol"}`)...)
	body = append(body, responsesFrame("response.output_item.added", 1, `"output_index":0,"item":{"id":"fc_1","type":"function_call","call_id":"call_1","name":"echo","arguments":""}`)...)
	body = append(body, responsesFrame("response.function_call_arguments.delta", 2, `"output_index":0,"item_id":"fc_1","call_id":"call_1","delta":"{\"text\":"`)...)
	body = append(body, responsesFrame("response.function_call_arguments.delta", 3, `"output_index":0,"item_id":"fc_1","call_id":"call_1","delta":"\"ping\"}"`)...)
	body = append(body, responsesFrame("response.function_call_arguments.done", 4, `"output_index":0,"item_id":"fc_1","call_id":"call_1"`)...)
	body = append(body, responsesFrame("response.completed", 5, `"response":{"id":"resp_tool","model":"gpt-5.6-sol","status":"completed","output":[],"usage":{"input_tokens":4,"output_tokens":3}}`)...)

	aggregated, err := aggregateOfficialResponsesSSE(body, int64(len(body)+1))
	if err != nil {
		t.Fatal(err)
	}
	var response map[string]any
	_ = json.Unmarshal(aggregated, &response)
	tool := response["output"].([]any)[0].(map[string]any)
	if tool["type"] != "function_call" || tool["id"] != "fc_1" || tool["call_id"] != "call_1" || tool["name"] != "echo" || tool["arguments"] != `{"text":"ping"}` {
		t.Fatalf("tool=%#v body=%s", tool, aggregated)
	}
}

func TestAggregateOfficialResponsesSSEPrefersNonEmptyTerminalOutput(t *testing.T) {
	body := append([]byte{}, responsesFrame("response.output_text.delta", 0, `"delta":"ignored"`)...)
	body = append(body, responsesFrame("response.completed", 1, `"response":{"id":"resp_full","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"canonical"}]}]}`)...)
	aggregated, err := aggregateOfficialResponsesSSE(body, int64(len(body)))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(aggregated), "canonical") || strings.Contains(string(aggregated), "ignored") {
		t.Fatalf("terminal response was not preferred: %s", aggregated)
	}
}

func TestAggregateOfficialResponsesSSERejectsFailureIncompleteAndLimitWithoutEcho(t *testing.T) {
	secret := "upstream-secret-must-not-escape"
	tests := map[string][]byte{
		"failed":           responsesFrame("response.failed", 0, `"response":{"error":"`+secret+`"}`),
		"error":            responsesFrame("error", 0, `"message":"`+secret+`"`),
		"no terminal":      responsesFrame("response.output_text.delta", 0, `"delta":"`+secret+`"`),
		"incomplete frame": []byte("event: response.completed\ndata: {\"type\":\"response.completed\",\"sequence_number\":0,\"response\":{\"output\":[]}}"),
	}
	for name, body := range tests {
		t.Run(name, func(t *testing.T) {
			_, err := aggregateOfficialResponsesSSE(body, int64(len(body)+1))
			if err == nil || strings.Contains(err.Error(), secret) {
				t.Fatalf("unsafe error: %v", err)
			}
		})
	}
	valid := responsesFrame("response.completed", 0, `"response":{"output":[]}`)
	if _, err := aggregateOfficialResponsesSSE(valid, int64(len(valid)-1)); err == nil {
		t.Fatal("accepted over-limit body")
	}
}
