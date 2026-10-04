package main

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"sort"
	"strconv"
	"strings"
)

// responsesChatError is deliberately safe to return to a local client: it never
// contains request content or an upstream response body.
type responsesChatError struct {
	code    string
	message string
}

func (e *responsesChatError) Error() string { return e.message }

func invalidResponses(message string) error {
	return &responsesChatError{code: "invalid_request", message: message}
}

func unsupportedResponses(code, message string) error {
	return &responsesChatError{code: code, message: message}
}

func invalidChatResponse() error {
	return &responsesChatError{code: "invalid_upstream_response", message: "The upstream response was invalid."}
}

// encodeResponsesToChat validates the supported Responses subset and renders an
// OpenAI-compatible Chat Completions request. Unknown fields are not forwarded.
func encodeResponsesToChat(body []byte) ([]byte, bool, error) {
	root, err := decodeJSONObject(body)
	if err != nil {
		return nil, false, invalidResponses("The request body must be a JSON object.")
	}
	model, err := requiredString(root, "model", "A non-empty model is required.")
	if err != nil {
		return nil, false, err
	}
	input, ok := root["input"]
	if !ok {
		return nil, false, invalidResponses("`input` is required.")
	}
	messages, err := responsesInputToChat(input)
	if err != nil {
		return nil, false, err
	}
	if raw, ok := root["instructions"]; ok {
		instructions, valid := raw.(string)
		if raw != nil && !valid {
			return nil, false, invalidResponses("`instructions` must be a string.")
		}
		if raw != nil {
			messages = append([]any{map[string]any{"role": "system", "content": instructions}}, messages...)
		}
	}
	stream := false
	if raw, ok := root["stream"]; ok {
		var valid bool
		stream, valid = raw.(bool)
		if !valid {
			return nil, false, invalidResponses("`stream` must be a boolean.")
		}
	}
	out := map[string]any{"model": model, "messages": messages, "stream": stream}
	if stream {
		out["stream_options"] = map[string]any{"include_usage": true}
	}
	if raw, ok := root["tools"]; ok {
		tools, err := responsesToolsToChat(raw)
		if err != nil {
			return nil, false, err
		}
		if len(tools) > 0 {
			out["tools"] = tools
		}
	}
	if raw, ok := root["tool_choice"]; ok {
		choice, err := responsesToolChoiceToChat(raw)
		if err != nil {
			return nil, false, err
		}
		if choice != nil {
			out["tool_choice"] = choice
		}
	}
	for _, key := range []string{"top_p", "presence_penalty", "frequency_penalty", "seed", "response_format", "n"} {
		if value, ok := root[key]; ok {
			out[key] = value
		}
	}
	if value, ok := root["max_output_tokens"]; ok {
		out["max_tokens"] = value
	}
	encoded, err := json.Marshal(out)
	if err != nil {
		return nil, false, invalidResponses("The request could not be encoded.")
	}
	return encoded, stream, nil
}

func responsesInputToChat(input any) ([]any, error) {
	if text, ok := input.(string); ok {
		return []any{map[string]any{"role": "user", "content": text}}, nil
	}
	items, ok := input.([]any)
	if !ok {
		return nil, invalidResponses("`input` must be a string or an array of input messages.")
	}
	messages := make([]any, 0, len(items))
	pendingCalls := make([]any, 0)
	flushCalls := func() {
		if len(pendingCalls) == 0 {
			return
		}
		messages = append(messages, map[string]any{"role": "assistant", "content": nil, "tool_calls": pendingCalls})
		pendingCalls = nil
	}
	for _, raw := range items {
		item, ok := raw.(map[string]any)
		if !ok {
			return nil, invalidResponses("Every item in `input` must be an object.")
		}
		kind, _ := item["type"].(string)
		switch kind {
		case "function_call":
			callID, err := requiredString(item, "call_id", "Function calls require a call_id.")
			if err != nil {
				return nil, err
			}
			name, err := requiredString(item, "name", "Function calls require a name.")
			if err != nil {
				return nil, err
			}
			arguments, err := requiredString(item, "arguments", "Function calls require string arguments.")
			if err != nil {
				return nil, err
			}
			pendingCalls = append(pendingCalls, map[string]any{"id": callID, "type": "function", "function": map[string]any{"name": name, "arguments": arguments}})
		case "function_call_output":
			flushCalls()
			callID, err := requiredString(item, "call_id", "Function call output requires a call_id.")
			if err != nil {
				return nil, err
			}
			value, ok := item["output"]
			if !ok {
				return nil, invalidResponses("Function call output requires an output value.")
			}
			output, err := responsesTextContent(value, true)
			if err != nil {
				return nil, err
			}
			messages = append(messages, map[string]any{"role": "tool", "tool_call_id": callID, "content": output})
		case "", "message":
			flushCalls()
			message, err := responsesMessageToChat(item)
			if err != nil {
				return nil, err
			}
			messages = append(messages, message)
		case "input_image", "image", "computer_screenshot":
			return nil, unsupportedResponses("unsupported_image_input", "Image input is not supported by this route.")
		case "input_file", "file", "input_audio", "audio", "item_reference":
			return nil, unsupportedResponses("unsupported_input", "This input item type is not supported by this route.")
		default:
			return nil, unsupportedResponses("unsupported_input", "This input item type is not supported by this route.")
		}
	}
	flushCalls()
	return messages, nil
}

func responsesMessageToChat(item map[string]any) (map[string]any, error) {
	role, err := requiredString(item, "role", "Every input message requires a role.")
	if err != nil {
		return nil, err
	}
	if role == "developer" {
		role = "system"
	}
	if role != "system" && role != "user" && role != "assistant" {
		return nil, invalidResponses("The input message role is not supported.")
	}
	content, ok := item["content"]
	if !ok {
		content = []any{}
	}
	text, err := responsesTextContent(content, false)
	if err != nil {
		return nil, err
	}
	message := map[string]any{"role": role, "content": text}
	if name, ok := item["name"]; ok {
		value, valid := name.(string)
		if name != nil && !valid {
			return nil, invalidResponses("Message name must be a string.")
		}
		if name != nil {
			message["name"] = value
		}
	}
	return message, nil
}

func responsesTextContent(value any, toolOutput bool) (string, error) {
	if text, ok := value.(string); ok {
		return text, nil
	}
	parts, ok := value.([]any)
	if !ok {
		if toolOutput {
			return "", invalidResponses("Function call output must be a string or an array of text content.")
		}
		return "", invalidResponses("Message content must be a string or an array of text content.")
	}
	var text strings.Builder
	for _, raw := range parts {
		part, ok := raw.(map[string]any)
		if !ok {
			return "", invalidResponses("Every content part must be an object.")
		}
		kind, err := requiredString(part, "type", "Every content part requires a type.")
		if err != nil {
			return "", err
		}
		switch kind {
		case "input_text", "output_text", "text":
			partText, ok := part["text"].(string)
			if !ok {
				return "", invalidResponses("Text content requires a text value.")
			}
			text.WriteString(partText)
		case "refusal":
			if toolOutput {
				return "", unsupportedResponses("unsupported_function_output_content", "This function call output content type is not supported by this route.")
			}
			partText, ok := part["text"].(string)
			if !ok {
				return "", invalidResponses("Refusal content requires a refusal value.")
			}
			text.WriteString(partText)
		case "input_image", "image", "computer_screenshot":
			return "", unsupportedResponses("unsupported_image_input", "Image input is not supported by this route.")
		case "input_file", "file", "input_audio", "audio", "item_reference":
			return "", unsupportedResponses("unsupported_input", "This content type is not supported by this route.")
		default:
			return "", unsupportedResponses("unsupported_input", "This content type is not supported by this route.")
		}
	}
	return text.String(), nil
}

func responsesToolsToChat(value any) ([]any, error) {
	items, ok := value.([]any)
	if !ok {
		return nil, invalidResponses("`tools` must be an array.")
	}
	tools := make([]any, 0, len(items))
	for _, raw := range items {
		tool, ok := raw.(map[string]any)
		if !ok {
			return nil, invalidResponses("Every tool must be an object.")
		}
		kind, err := requiredString(tool, "type", "Every tool requires a type.")
		if err != nil {
			return nil, err
		}
		if kind != "function" {
			continue
		}
		name, err := requiredString(tool, "name", "Function tools require a name.")
		if err != nil {
			return nil, err
		}
		parameters := any(map[string]any{"type": "object", "properties": map[string]any{}})
		if rawParameters, ok := tool["parameters"]; ok {
			if _, ok := rawParameters.(map[string]any); !ok {
				return nil, invalidResponses("Function tool parameters must be a JSON object.")
			}
			parameters = rawParameters
		}
		function := map[string]any{"name": name, "parameters": parameters}
		if description, ok := tool["description"]; ok {
			value, valid := description.(string)
			if description != nil && !valid {
				return nil, invalidResponses("Function tool description must be a string.")
			}
			if description != nil {
				function["description"] = value
			}
		}
		if strict, ok := tool["strict"]; ok {
			value, ok := strict.(bool)
			if !ok {
				return nil, invalidResponses("Tool strict must be a boolean.")
			}
			function["strict"] = value
		}
		tools = append(tools, map[string]any{"type": "function", "function": function})
	}
	return tools, nil
}

func responsesToolChoiceToChat(value any) (any, error) {
	if choice, ok := value.(string); ok {
		if choice == "auto" || choice == "none" || choice == "required" {
			return choice, nil
		}
		return nil, invalidResponses("`tool_choice` is invalid.")
	}
	choice, ok := value.(map[string]any)
	if !ok {
		return nil, invalidResponses("`tool_choice` must be a string or an object.")
	}
	if choice["type"] != "function" {
		return nil, nil
	}
	name, _ := choice["name"].(string)
	if name == "" {
		if function, ok := choice["function"].(map[string]any); ok {
			name, _ = function["name"].(string)
		}
	}
	if strings.TrimSpace(name) == "" {
		return nil, invalidResponses("Function tool choice requires a name.")
	}
	return map[string]any{"type": "function", "function": map[string]any{"name": name}}, nil
}

// translateChatJSONToResponses translates one successful Chat Completions JSON
// object. Upstream error objects and malformed payloads become a fixed safe error.
func translateChatJSONToResponses(body []byte) ([]byte, error) {
	root, err := decodeJSONObject(body)
	if err != nil || reportedError(root["error"]) {
		return nil, invalidChatResponse()
	}
	choices, ok := root["choices"].([]any)
	if !ok || len(choices) == 0 {
		return nil, invalidChatResponse()
	}
	choice, ok := choices[0].(map[string]any)
	if !ok {
		return nil, invalidChatResponse()
	}
	message, ok := choice["message"].(map[string]any)
	if !ok {
		return nil, invalidChatResponse()
	}
	id := responseID(root)
	output := make([]any, 0)
	if content, ok := message["content"].(string); ok {
		output = append(output, responseMessageItem("msg_"+id, content, "completed"))
	}
	if calls, ok := message["tool_calls"].([]any); ok {
		for index, raw := range calls {
			item, err := responseFunctionItem(raw, index)
			if err != nil {
				return nil, err
			}
			output = append(output, item)
		}
	} else if raw, ok := message["function_call"]; ok {
		item, err := legacyResponseFunctionItem(raw)
		if err != nil {
			return nil, err
		}
		output = append(output, item)
	}
	finish, _ := choice["finish_reason"].(string)
	response := responsesObject(id, stringValue(root["model"], "unknown"), uintValue(root["created"]), output, finish, chatUsage(root["usage"]), true)
	encoded, err := json.Marshal(response)
	if err != nil {
		return nil, invalidChatResponse()
	}
	return encoded, nil
}

type chatToolState struct {
	sourceIndex int
	outputIndex int
	itemID      string
	callID      string
	name        string
	arguments   string
}

// chatToResponsesSSE consumes individual Chat SSE data payloads (without the
// `data:` prefix) and returns complete Responses SSE records.
type chatToResponsesSSE struct {
	responseID string
	model      string
	created    uint64
	started    bool
	completed  bool
	failed     bool
	sequence   uint64
	nextOutput int
	message    *chatMessageState
	tools      map[int]*chatToolState
	usage      map[string]any
	finishWhy  string
}

type chatMessageState struct {
	outputIndex int
	itemID      string
	text        string
	partAdded   bool
}

func newChatToResponsesSSE(model string) *chatToResponsesSSE {
	return &chatToResponsesSSE{responseID: newResponseID(), model: model, tools: make(map[int]*chatToolState)}
}

// consume returns translated frames and true once [DONE] has completed the stream.
func (s *chatToResponsesSSE) consume(data []byte) ([]byte, bool, error) {
	if s.completed {
		return nil, true, nil
	}
	if strings.TrimSpace(string(data)) == "[DONE]" {
		return s.finish(), true, nil
	}
	root, err := decodeJSONObject(data)
	if err != nil || reportedError(root["error"]) {
		return nil, false, invalidChatResponse()
	}
	if value, ok := jsonUint(root["created"]); ok {
		s.created = value
	}
	if value, ok := root["model"].(string); ok && value != "" {
		s.model = value
	}
	if usage := chatUsage(root["usage"]); usage != nil {
		s.usage = usage
	}
	events := s.ensureStarted()
	choices, _ := root["choices"].([]any)
	if len(choices) > 0 {
		choice, ok := choices[0].(map[string]any)
		if !ok {
			return nil, false, invalidChatResponse()
		}
		if delta, ok := choice["delta"].(map[string]any); ok {
			if content, ok := delta["content"].(string); ok && content != "" {
				events = append(events, s.appendText(content)...)
			}
			if calls, ok := delta["tool_calls"].([]any); ok {
				for position, raw := range calls {
					callEvents, err := s.appendTool(raw, position)
					if err != nil {
						return nil, false, err
					}
					events = append(events, callEvents...)
				}
			}
			if raw, ok := delta["function_call"]; ok {
				callEvents, err := s.appendTool(map[string]any{"index": json.Number("0"), "function": raw}, 0)
				if err != nil {
					return nil, false, err
				}
				events = append(events, callEvents...)
			}
		}
		if reason, ok := choice["finish_reason"].(string); ok {
			s.finishWhy = reason
		}
	}
	return marshalSSEEvents(events), false, nil
}

func (s *chatToResponsesSSE) finish() []byte {
	if s.completed {
		return nil
	}
	events := s.ensureStarted()
	s.completed = true
	if s.message != nil {
		message := s.message
		if message.partAdded {
			events = append(events,
				s.event("response.output_text.done", map[string]any{"output_index": message.outputIndex, "item_id": message.itemID, "content_index": 0, "text": message.text}),
				s.event("response.content_part.done", map[string]any{"output_index": message.outputIndex, "item_id": message.itemID, "content_index": 0, "part": responseTextPart(message.text)}))
		}
		events = append(events, s.event("response.output_item.done", map[string]any{"output_index": message.outputIndex, "item": responseMessageItem(message.itemID, message.text, "completed")}))
	}
	indices := make([]int, 0, len(s.tools))
	for index := range s.tools {
		indices = append(indices, index)
	}
	sort.Ints(indices)
	for _, index := range indices {
		tool := s.tools[index]
		events = append(events,
			s.event("response.function_call_arguments.done", map[string]any{"output_index": tool.outputIndex, "item_id": tool.itemID, "call_id": tool.callID, "arguments": tool.arguments}),
			s.event("response.output_item.done", map[string]any{"output_index": tool.outputIndex, "item": tool.item("completed")}))
	}
	name := "response.completed"
	if s.finishWhy == "length" || s.finishWhy == "content_filter" {
		name = "response.incomplete"
	}
	events = append(events, s.event(name, map[string]any{"response": s.responseObject(true)}))
	return marshalSSEEvents(events)
}

func (s *chatToResponsesSSE) fail() []byte {
	if s.failed {
		return nil
	}
	s.failed = true
	s.completed = true
	return marshalSSEEvents([]sseEvent{s.event("error", map[string]any{
		"code":    "upstream_error",
		"message": "The upstream model provider returned an invalid stream.",
		"param":   nil,
	})})
}

func (s *chatToResponsesSSE) ensureStarted() []sseEvent {
	if s.started {
		return nil
	}
	s.started = true
	response := s.responseObject(false)
	return []sseEvent{
		s.event("response.created", map[string]any{"response": response}),
		s.event("response.in_progress", map[string]any{"response": response}),
	}
}

func (s *chatToResponsesSSE) appendText(delta string) []sseEvent {
	events := make([]sseEvent, 0, 3)
	if s.message == nil {
		index := s.allocateOutput()
		s.message = &chatMessageState{outputIndex: index, itemID: "msg_" + s.responseID}
		events = append(events, s.event("response.output_item.added", map[string]any{"output_index": index, "item": responseMessageItem(s.message.itemID, "", "in_progress")}))
	}
	if !s.message.partAdded {
		s.message.partAdded = true
		events = append(events, s.event("response.content_part.added", map[string]any{"output_index": s.message.outputIndex, "item_id": s.message.itemID, "content_index": 0, "part": responseTextPart("")}))
	}
	s.message.text += delta
	events = append(events, s.event("response.output_text.delta", map[string]any{"output_index": s.message.outputIndex, "item_id": s.message.itemID, "content_index": 0, "delta": delta}))
	return events
}

func (s *chatToResponsesSSE) appendTool(raw any, fallback int) ([]sseEvent, error) {
	call, ok := raw.(map[string]any)
	if !ok {
		return nil, invalidChatResponse()
	}
	index := fallback
	if value, ok := jsonUint(call["index"]); ok {
		index = int(value)
	}
	function, _ := call["function"].(map[string]any)
	name, _ := function["name"].(string)
	arguments, _ := function["arguments"].(string)
	events := make([]sseEvent, 0, 2)
	tool := s.tools[index]
	if tool == nil {
		callID, _ := call["id"].(string)
		if callID == "" {
			callID = fmt.Sprintf("call_%d", index)
		}
		tool = &chatToolState{sourceIndex: index, outputIndex: s.allocateOutput(), itemID: "fc_" + callID, callID: callID, name: name}
		s.tools[index] = tool
		events = append(events, s.event("response.output_item.added", map[string]any{"output_index": tool.outputIndex, "item": tool.item("in_progress")}))
	}
	if name != "" {
		tool.name = name
	}
	if arguments != "" {
		tool.arguments += arguments
		events = append(events, s.event("response.function_call_arguments.delta", map[string]any{"output_index": tool.outputIndex, "item_id": tool.itemID, "call_id": tool.callID, "delta": arguments}))
	}
	return events, nil
}

func (s *chatToResponsesSSE) responseObject(completed bool) map[string]any {
	output := make([]any, 0, 1+len(s.tools))
	type indexed struct {
		index int
		item  any
	}
	indexedOutput := make([]indexed, 0, 1+len(s.tools))
	if s.message != nil {
		indexedOutput = append(indexedOutput, indexed{s.message.outputIndex, responseMessageItem(s.message.itemID, s.message.text, "completed")})
	}
	for _, tool := range s.tools {
		indexedOutput = append(indexedOutput, indexed{tool.outputIndex, tool.item("completed")})
	}
	sort.Slice(indexedOutput, func(i, j int) bool { return indexedOutput[i].index < indexedOutput[j].index })
	for _, entry := range indexedOutput {
		output = append(output, entry.item)
	}
	return responsesObject(s.responseID, s.model, s.created, output, s.finishWhy, s.usage, completed)
}

func (s *chatToResponsesSSE) event(name string, fields map[string]any) sseEvent {
	fields["type"] = name
	fields["sequence_number"] = s.sequence
	s.sequence++
	return sseEvent{name: name, data: fields}
}

func (s *chatToResponsesSSE) allocateOutput() int {
	index := s.nextOutput
	s.nextOutput++
	return index
}

func (t *chatToolState) item(status string) map[string]any {
	return map[string]any{"id": t.itemID, "type": "function_call", "status": status, "call_id": t.callID, "name": t.name, "arguments": t.arguments}
}

type sseEvent struct {
	name string
	data map[string]any
}

func marshalSSEEvents(events []sseEvent) []byte {
	var output bytes.Buffer
	for _, event := range events {
		encoded, err := json.Marshal(event.data)
		if err != nil {
			continue
		}
		fmt.Fprintf(&output, "event: %s\ndata: %s\n\n", event.name, encoded)
	}
	return output.Bytes()
}

func responsesObject(id, model string, created uint64, output []any, finish string, usage map[string]any, completed bool) map[string]any {
	status := "in_progress"
	var incomplete any
	if completed {
		status = "completed"
		if finish == "length" || finish == "content_filter" {
			status = "incomplete"
			reason := "max_output_tokens"
			if finish == "content_filter" {
				reason = "content_filter"
			}
			incomplete = map[string]any{"reason": reason}
		}
	}
	if completed && usage == nil {
		usage = zeroResponsesUsage()
	}
	return map[string]any{"id": id, "object": "response", "created_at": created, "model": model, "output": output, "status": status, "incomplete_details": incomplete, "usage": usage}
}

func responseMessageItem(id, text, status string) map[string]any {
	return map[string]any{"id": id, "type": "message", "status": status, "role": "assistant", "content": []any{responseTextPart(text)}}
}

func responseTextPart(text string) map[string]any {
	return map[string]any{"type": "output_text", "text": text, "annotations": []any{}}
}

func responseFunctionItem(raw any, index int) (map[string]any, error) {
	call, ok := raw.(map[string]any)
	if !ok {
		return nil, invalidChatResponse()
	}
	function, ok := call["function"].(map[string]any)
	if !ok {
		return nil, invalidChatResponse()
	}
	name, ok := function["name"].(string)
	if !ok || name == "" {
		return nil, invalidChatResponse()
	}
	callID, _ := call["id"].(string)
	if callID == "" {
		callID = fmt.Sprintf("call_%d", index)
	}
	arguments, _ := function["arguments"].(string)
	return map[string]any{"id": "fc_" + callID, "type": "function_call", "status": "completed", "call_id": callID, "name": name, "arguments": arguments}, nil
}

func legacyResponseFunctionItem(raw any) (map[string]any, error) {
	function, ok := raw.(map[string]any)
	if !ok {
		return nil, invalidChatResponse()
	}
	name, ok := function["name"].(string)
	if !ok || name == "" {
		return nil, invalidChatResponse()
	}
	arguments, _ := function["arguments"].(string)
	return map[string]any{"id": "fc_call_0", "type": "function_call", "status": "completed", "call_id": "call_0", "name": name, "arguments": arguments}, nil
}

func chatUsage(raw any) map[string]any {
	usage, ok := raw.(map[string]any)
	if !ok {
		return nil
	}
	input, ok := jsonUint(usage["prompt_tokens"])
	if !ok {
		input, ok = jsonUint(usage["input_tokens"])
	}
	if !ok {
		return nil
	}
	output, outputFound := jsonUint(usage["completion_tokens"])
	if !outputFound {
		output, _ = jsonUint(usage["output_tokens"])
	}
	total, ok := jsonUint(usage["total_tokens"])
	if !ok {
		total = input + output
	}
	cached := uint64(0)
	if details, ok := usage["prompt_tokens_details"].(map[string]any); ok {
		cached, _ = jsonUint(details["cached_tokens"])
	}
	reasoning, reasoningFound := jsonUint(usage["reasoning_tokens"])
	if !reasoningFound {
		if details, ok := usage["completion_tokens_details"].(map[string]any); ok {
			reasoning, _ = jsonUint(details["reasoning_tokens"])
		} else if details, ok := usage["output_tokens_details"].(map[string]any); ok {
			reasoning, _ = jsonUint(details["reasoning_tokens"])
		}
	}
	return map[string]any{
		"input_tokens": input, "input_tokens_details": map[string]any{"cached_tokens": cached},
		"output_tokens": output, "output_tokens_details": map[string]any{"reasoning_tokens": reasoning},
		"total_tokens": total, "reasoning_tokens": reasoning,
	}
}

func zeroResponsesUsage() map[string]any {
	return map[string]any{
		"input_tokens": uint64(0), "input_tokens_details": map[string]any{"cached_tokens": uint64(0)},
		"output_tokens": uint64(0), "output_tokens_details": map[string]any{"reasoning_tokens": uint64(0)},
		"total_tokens": uint64(0), "reasoning_tokens": uint64(0),
	}
}

func decodeJSONObject(body []byte) (map[string]any, error) {
	decoder := json.NewDecoder(bytes.NewReader(body))
	decoder.UseNumber()
	var value any
	if err := decoder.Decode(&value); err != nil {
		return nil, err
	}
	var trailing any
	if err := decoder.Decode(&trailing); err == nil {
		return nil, errors.New("trailing JSON")
	} else if err != io.EOF {
		return nil, err
	}
	object, ok := value.(map[string]any)
	if !ok {
		return nil, errors.New("not an object")
	}
	return object, nil
}

func requiredString(object map[string]any, key, message string) (string, error) {
	value, ok := object[key].(string)
	if !ok || strings.TrimSpace(value) == "" {
		return "", invalidResponses(message)
	}
	return value, nil
}

func reportedError(value any) bool {
	if value == nil {
		return false
	}
	object, ok := value.(map[string]any)
	return !ok || len(object) > 0
}

func responseID(root map[string]any) string {
	if id, ok := root["id"].(string); ok && id != "" {
		return id
	}
	return newResponseID()
}

func newResponseID() string {
	raw := make([]byte, 12)
	if _, err := rand.Read(raw); err != nil {
		return "resp_agenthub"
	}
	return "resp_" + hex.EncodeToString(raw)
}

func jsonUint(value any) (uint64, bool) {
	switch typed := value.(type) {
	case json.Number:
		output, err := strconv.ParseUint(typed.String(), 10, 64)
		return output, err == nil
	case float64:
		if typed >= 0 && typed == float64(uint64(typed)) {
			return uint64(typed), true
		}
	case uint64:
		return typed, true
	case int:
		if typed >= 0 {
			return uint64(typed), true
		}
	}
	return 0, false
}

func uintValue(value any) uint64 {
	result, _ := jsonUint(value)
	return result
}

func stringValue(value any, fallback string) string {
	if result, ok := value.(string); ok && result != "" {
		return result
	}
	return fallback
}
