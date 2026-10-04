package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"sort"
	"strconv"
	"strings"
)

var errInvalidResponsesSSE = errors.New("upstream Responses stream is invalid")
var errOfficialResponsesFailure = errors.New("upstream Responses stream reported failure")

type responsesSSEEvent struct {
	Type           string
	SequenceNumber uint64
	Terminal       bool
	ErrorLike      bool
	Value          map[string]any
}

// responsesSSEState validates one already-delimited SSE frame at a time.
// Empty/comment-only frames are accepted and return a nil event.
type responsesSSEState struct {
	lastSequence uint64
	hasSequence  bool
	terminal     bool
}

func newResponsesSSEState() *responsesSSEState {
	return &responsesSSEState{}
}

func (state *responsesSSEState) consumeFrame(frame []byte) (*responsesSSEEvent, error) {
	eventName, payload, hasData, err := parseResponsesSSEFields(frame)
	if err != nil {
		return nil, errInvalidResponsesSSE
	}
	if !hasData || len(payload) == 0 {
		return nil, nil
	}
	if bytes.Equal(payload, []byte("[DONE]")) || state.terminal {
		return nil, errInvalidResponsesSSE
	}

	decoder := json.NewDecoder(bytes.NewReader(payload))
	decoder.UseNumber()
	var value map[string]any
	if err := decoder.Decode(&value); err != nil || value == nil {
		return nil, errInvalidResponsesSSE
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return nil, errInvalidResponsesSSE
	}
	kind, ok := value["type"].(string)
	if !ok || eventName != kind || !isKnownResponsesEventType(kind) {
		return nil, errInvalidResponsesSSE
	}
	sequence, ok := responsesSequenceNumber(value["sequence_number"])
	if !ok || sequence == ^uint64(0) || (state.hasSequence && sequence <= state.lastSequence) {
		return nil, errInvalidResponsesSSE
	}
	state.lastSequence = sequence
	state.hasSequence = true
	terminal := isResponsesTerminalEvent(kind)
	state.terminal = terminal
	return &responsesSSEEvent{
		Type:           kind,
		SequenceNumber: sequence,
		Terminal:       terminal,
		ErrorLike:      kind == "response.failed" || kind == "error",
		Value:          value,
	}, nil
}

func (state *responsesSSEState) finish() error {
	if state == nil || !state.terminal {
		return errInvalidResponsesSSE
	}
	return nil
}

func parseResponsesSSEFields(frame []byte) (string, []byte, bool, error) {
	normalized := strings.ReplaceAll(strings.ReplaceAll(string(frame), "\r\n", "\n"), "\r", "\n")
	var eventNames []string
	var dataLines []string
	for _, line := range strings.Split(normalized, "\n") {
		if line == "" || strings.HasPrefix(line, ":") {
			continue
		}
		field, value, found := strings.Cut(line, ":")
		if !found {
			value = ""
		}
		value = strings.TrimPrefix(value, " ")
		switch field {
		case "event":
			eventNames = append(eventNames, value)
		case "data":
			dataLines = append(dataLines, value)
		}
	}
	if len(dataLines) == 0 {
		return "", nil, false, nil
	}
	if len(eventNames) != 1 || eventNames[0] == "" {
		return "", nil, false, errInvalidResponsesSSE
	}
	return eventNames[0], []byte(strings.Join(dataLines, "\n")), true, nil
}

func responsesSequenceNumber(value any) (uint64, bool) {
	switch number := value.(type) {
	case json.Number:
		if strings.ContainsAny(number.String(), ".eE+-") {
			return 0, false
		}
		parsed, err := strconv.ParseUint(number.String(), 10, 64)
		return parsed, err == nil
	case float64:
		if number < 0 || number > float64(^uint64(0)) || number != float64(uint64(number)) {
			return 0, false
		}
		return uint64(number), true
	case uint64:
		return number, true
	case int:
		return uint64(number), number >= 0
	default:
		return 0, false
	}
}

func isResponsesTerminalEvent(kind string) bool {
	return kind == "response.completed" || kind == "response.incomplete" ||
		kind == "response.failed" || kind == "error"
}

func isKnownResponsesEventType(kind string) bool {
	switch kind {
	case "error",
		"response.audio.delta", "response.audio.done",
		"response.audio.transcript.delta", "response.audio.transcript.done",
		"response.code_interpreter_call_code.delta", "response.code_interpreter_call_code.done",
		"response.code_interpreter_call.completed", "response.code_interpreter_call.in_progress", "response.code_interpreter_call.interpreting",
		"response.completed", "response.content_part.added", "response.content_part.done",
		"response.created", "response.custom_tool_call_input.delta", "response.custom_tool_call_input.done",
		"response.failed", "response.file_search_call.completed", "response.file_search_call.in_progress", "response.file_search_call.searching",
		"response.function_call_arguments.delta", "response.function_call_arguments.done",
		"response.image_generation_call.completed", "response.image_generation_call.generating", "response.image_generation_call.in_progress", "response.image_generation_call.partial_image",
		"response.in_progress", "response.incomplete",
		"response.mcp_call_arguments.delta", "response.mcp_call_arguments.done",
		"response.mcp_call.completed", "response.mcp_call.failed", "response.mcp_call.in_progress",
		"response.mcp_list_tools.completed", "response.mcp_list_tools.failed", "response.mcp_list_tools.in_progress",
		"response.output_item.added", "response.output_item.done",
		"response.output_text.annotation.added", "response.output_text.delta", "response.output_text.done",
		"response.queued",
		"response.reasoning_summary_part.added", "response.reasoning_summary_part.done",
		"response.reasoning_summary_text.delta", "response.reasoning_summary_text.done",
		"response.reasoning_text.delta", "response.reasoning_text.done",
		"response.refusal.delta", "response.refusal.done",
		"response.shell_call.command.added", "response.shell_call.command.delta", "response.shell_call.command.done",
		"response.shell_call.output_content.delta", "response.shell_call.output_content.done",
		"response.text.delta", "response.text.done",
		"response.tool_search_call.completed", "response.tool_search_call.failed", "response.tool_search_call.in_progress",
		"response.web_search_call.completed", "response.web_search_call.in_progress", "response.web_search_call.searching":
		return true
	default:
		return false
	}
}

// aggregateOfficialResponsesSSE validates and aggregates one complete official
// Responses SSE body. maxBytes must be positive and bounds the supplied body.
// Returned errors are deliberately generic and never include upstream data.
func aggregateOfficialResponsesSSE(body []byte, maxBytes int64) ([]byte, error) {
	if maxBytes <= 0 || int64(len(body)) > maxBytes {
		return nil, errInvalidResponsesSSE
	}
	frames, err := splitCompleteResponsesSSEFrames(body)
	if err != nil {
		return nil, err
	}
	state := newResponsesSSEState()
	aggregate := responsesSSEAggregate{toolsByKey: make(map[string]*responsesSSETool), textIndexes: make(map[int]struct{})}
	for _, frame := range frames {
		event, err := state.consumeFrame(frame)
		if err != nil {
			return nil, err
		}
		if event == nil {
			continue
		}
		if event.ErrorLike {
			return nil, errOfficialResponsesFailure
		}
		if err := aggregate.consume(event); err != nil {
			return nil, errInvalidResponsesSSE
		}
	}
	if err := state.finish(); err != nil || aggregate.terminalResponse == nil {
		return nil, errInvalidResponsesSSE
	}
	response, err := aggregate.response()
	if err != nil {
		return nil, errInvalidResponsesSSE
	}
	encoded, err := json.Marshal(response)
	if err != nil {
		return nil, errInvalidResponsesSSE
	}
	return encoded, nil
}

func splitCompleteResponsesSSEFrames(body []byte) ([][]byte, error) {
	normalized := strings.ReplaceAll(strings.ReplaceAll(string(body), "\r\n", "\n"), "\r", "\n")
	frames := make([][]byte, 0, 8)
	for {
		index := strings.Index(normalized, "\n\n")
		if index < 0 {
			break
		}
		frames = append(frames, []byte(normalized[:index]))
		normalized = normalized[index+2:]
	}
	if !legalResponsesSSETrailer(normalized) {
		return nil, errInvalidResponsesSSE
	}
	return frames, nil
}

func legalResponsesSSETrailer(trailer string) bool {
	for _, line := range strings.Split(trailer, "\n") {
		if strings.TrimSpace(line) == "" || strings.HasPrefix(line, ":") {
			continue
		}
		return false
	}
	return true
}

type responsesSSEAggregate struct {
	text             strings.Builder
	terminalResponse map[string]any
	tools            []*responsesSSETool
	toolsByKey       map[string]*responsesSSETool
	textIndexes      map[int]struct{}
	nextToolOrder    int
}

type responsesSSETool struct {
	key       string
	itemID    string
	callID    string
	name      string
	arguments strings.Builder
	index     int
	order     int
}

func (aggregate *responsesSSEAggregate) consume(event *responsesSSEEvent) error {
	switch event.Type {
	case "response.output_text.delta":
		if delta, ok := event.Value["delta"].(string); ok {
			aggregate.text.WriteString(delta)
			aggregate.textIndexes[responsesOutputIndex(event.Value["output_index"])] = struct{}{}
		}
	case "response.output_item.added", "response.output_item.done":
		item, _ := event.Value["item"].(map[string]any)
		if item != nil && item["type"] == "function_call" {
			tool := aggregate.toolFor(event.Value, item)
			aggregate.mergeTool(tool, item)
		}
	case "response.function_call_arguments.delta":
		tool := aggregate.toolFor(event.Value, nil)
		if delta, ok := event.Value["delta"].(string); ok {
			tool.arguments.WriteString(delta)
		}
	case "response.function_call_arguments.done":
		tool := aggregate.toolFor(event.Value, nil)
		if tool.arguments.Len() == 0 {
			if arguments, ok := event.Value["arguments"].(string); ok {
				tool.arguments.WriteString(arguments)
			}
		}
	case "response.completed", "response.incomplete":
		response, ok := event.Value["response"].(map[string]any)
		if !ok {
			return errInvalidResponsesSSE
		}
		aggregate.terminalResponse = cloneResponsesObject(response)
	}
	return nil
}

func (aggregate *responsesSSEAggregate) toolFor(event map[string]any, item map[string]any) *responsesSSETool {
	key := firstResponsesString(event, "call_id", "item_id")
	if key == "" && item != nil {
		key = firstResponsesString(item, "call_id", "id")
	}
	index := responsesOutputIndex(event["output_index"])
	if key == "" && index >= 0 {
		key = "index:" + strconv.Itoa(index)
	}
	if key == "" {
		key = "call_0"
	}
	if tool := aggregate.toolsByKey[key]; tool != nil {
		return tool
	}
	tool := &responsesSSETool{key: key, index: index, order: aggregate.nextToolOrder}
	aggregate.nextToolOrder++
	aggregate.toolsByKey[key] = tool
	aggregate.tools = append(aggregate.tools, tool)
	return tool
}

func (aggregate *responsesSSEAggregate) mergeTool(tool *responsesSSETool, item map[string]any) {
	if value, ok := item["id"].(string); ok && value != "" {
		tool.itemID = value
	}
	if value, ok := item["call_id"].(string); ok && value != "" {
		tool.callID = value
	}
	if value, ok := item["name"].(string); ok && value != "" {
		tool.name = value
	}
	if tool.arguments.Len() == 0 {
		if value, ok := item["arguments"].(string); ok && value != "" {
			tool.arguments.WriteString(value)
		}
	}
}

func (aggregate *responsesSSEAggregate) response() (map[string]any, error) {
	response := aggregate.terminalResponse
	if output, ok := response["output"].([]any); ok && len(output) > 0 {
		return response, nil
	}
	if aggregate.text.Len() > 0 && (len(aggregate.tools) > 0 || len(aggregate.textIndexes) > 1) {
		return nil, errInvalidResponsesSSE
	}
	output := make([]any, 0, 1+len(aggregate.tools))
	if aggregate.text.Len() > 0 {
		responseID, _ := response["id"].(string)
		messageID := "msg_agenthub"
		if responseID != "" {
			messageID = "msg_" + responseID
		}
		output = append(output, map[string]any{
			"id":     messageID,
			"type":   "message",
			"status": "completed",
			"role":   "assistant",
			"content": []any{map[string]any{
				"type":        "output_text",
				"text":        aggregate.text.String(),
				"annotations": []any{},
			}},
		})
	}
	sort.SliceStable(aggregate.tools, func(left, right int) bool {
		leftIndex := aggregate.tools[left].index
		rightIndex := aggregate.tools[right].index
		if leftIndex >= 0 && rightIndex >= 0 && leftIndex != rightIndex {
			return leftIndex < rightIndex
		}
		return aggregate.tools[left].order < aggregate.tools[right].order
	})
	for _, tool := range aggregate.tools {
		callID := tool.callID
		if callID == "" {
			callID = tool.key
			if strings.HasPrefix(callID, "index:") {
				callID = "call_" + strings.TrimPrefix(callID, "index:")
			}
		}
		name := tool.name
		if name == "" {
			name = "tool"
		}
		itemID := tool.itemID
		if itemID == "" {
			itemID = "fc_" + callID
		}
		output = append(output, map[string]any{
			"id":        itemID,
			"type":      "function_call",
			"status":    "completed",
			"call_id":   callID,
			"name":      name,
			"arguments": tool.arguments.String(),
		})
	}
	response["output"] = output
	return response, nil
}

func firstResponsesString(value map[string]any, keys ...string) string {
	for _, key := range keys {
		if text, ok := value[key].(string); ok && text != "" {
			return text
		}
	}
	return ""
}

func responsesOutputIndex(value any) int {
	sequence, ok := responsesSequenceNumber(value)
	if !ok || sequence > uint64(^uint(0)>>1) {
		return -1
	}
	return int(sequence)
}

func cloneResponsesObject(source map[string]any) map[string]any {
	clone := make(map[string]any, len(source))
	for key, value := range source {
		clone[key] = value
	}
	return clone
}
