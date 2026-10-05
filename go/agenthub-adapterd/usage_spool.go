package main

import (
	"bytes"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// gatewayUsageEvent is intentionally the same JSONL wire shape consumed by
// core's GatewayUsageEvent. Keep this narrow: source identities, tickets,
// sessions, URLs, request/response payloads, and login material do not belong
// in the sidecar spool.
type gatewayUsageEvent struct {
	RequestID         string  `json:"request_id"`
	Timestamp         string  `json:"ts"`
	ProfileID         string  `json:"profile_id"`
	Surface           string  `json:"surface"`
	UpstreamChannel   *string `json:"upstream_channel,omitempty"`
	TicketID          *string `json:"ticket_id,omitempty"`
	AccountKind       *string `json:"account_source_kind,omitempty"`
	AccountID         *string `json:"account_source_id,omitempty"`
	Model             *string `json:"model,omitempty"`
	UpstreamModel     *string `json:"upstream_model,omitempty"`
	InputTokens       uint64  `json:"input_tokens"`
	OutputTokens      uint64  `json:"output_tokens"`
	CachedInputTokens *uint64 `json:"cached_input_tokens,omitempty"`
	ReasoningTokens   *uint64 `json:"reasoning_tokens,omitempty"`
	Status            string  `json:"status"`
	StatusCode        *uint16 `json:"status_code,omitempty"`
	ErrorClass        *string `json:"error_class,omitempty"`
	LatencyMS         *uint64 `json:"latency_ms,omitempty"`
	TTFTMS            *uint64 `json:"ttft_ms,omitempty"`
	Attempts          *uint32 `json:"attempts,omitempty"`
}

// usageSpool owns exactly one append writer for its configured directory.
// Runtime config swaps reuse it when the path is unchanged, so requests before
// and after a Product reload never interleave through separate file handles.
type usageSpool struct {
	dir  string
	mu   sync.Mutex
	day  string
	file *os.File
}

func newUsageSpool(dir string) *usageSpool {
	return &usageSpool{dir: dir}
}

// record serializes and writes one complete JSONL row. Every failure is
// deliberately represented only by false: the caller logs a fixed local code
// and forwarding continues without exposing filesystem details or input data.
func (spool *usageSpool) record(event gatewayUsageEvent) bool {
	if spool == nil {
		return true
	}
	line, err := json.Marshal(event)
	if err != nil {
		return false
	}
	line = append(line, '\n')
	day := usageSpoolDay(event.Timestamp)

	spool.mu.Lock()
	defer spool.mu.Unlock()
	if spool.day != day || spool.file == nil {
		if spool.file != nil {
			_ = spool.file.Close()
			spool.file = nil
		}
		if err := os.MkdirAll(spool.dir, 0o700); err != nil {
			return false
		}
		file, err := os.OpenFile(
			filepath.Join(spool.dir, "gateway-"+day+".jsonl"),
			os.O_CREATE|os.O_APPEND|os.O_WRONLY,
			0o600,
		)
		if err != nil {
			return false
		}
		spool.day = day
		spool.file = file
	}
	if !writeUsageSpoolLine(spool.file, line) || spool.file.Sync() != nil {
		_ = spool.file.Close()
		spool.file = nil
		return false
	}
	return true
}

func writeUsageSpoolLine(file *os.File, line []byte) bool {
	for len(line) > 0 {
		count, err := file.Write(line)
		if err != nil || count == 0 {
			return false
		}
		line = line[count:]
	}
	return true
}

func usageSpoolDay(timestamp string) string {
	if len(timestamp) >= len("2006-01-02") {
		date := timestamp[:10]
		return date[0:4] + date[5:7] + date[8:10]
	}
	return "19700101"
}

type usageCapture struct {
	spool           *usageSpool
	requestID       string
	profileID       string
	surface         string
	started         time.Time
	model           *string
	upstreamModel   *string
	upstreamChannel *string
	ticketID        *string
	accountKind     *string
	accountID       *string
	attempts        uint32
	streaming       bool
	failureClass    string
	tokens          capturedGatewayUsage
	sse             usageSSECapture
}

// capturedGatewayUsage carries only validated numeric counters. Response
// payloads are decoded briefly at the protocol boundary and never retained in
// the spool, status, or sidecar log.
type capturedGatewayUsage struct {
	input     uint64
	output    uint64
	cached    *uint64
	reasoning *uint64
	valid     bool
}

// usageSSECapture stores a bounded incomplete frame while the response is in
// flight. It is cleared when the request completes; it must never become a
// durable part of a usage event.
type usageSSECapture struct {
	pending   []byte
	invalid   bool
	input     *uint64
	output    *uint64
	cached    *uint64
	reasoning *uint64
	chat      *capturedGatewayUsage
}

const maxUsageSSEFrameBytes = 64 << 10

func (rt *Runtime) beginUsageCapture(edge *RuntimeEdge, surface string) *usageCapture {
	if edge == nil || edge.ID == "" {
		return nil
	}
	rt.mu.Lock()
	spool := rt.usageSpool
	rt.mu.Unlock()
	if spool == nil {
		return nil
	}
	requestID, ok := newGatewayUsageUUIDv4()
	if !ok {
		rt.logf("usage capture dropped code=usage_spool_request_id_unavailable")
		return nil
	}
	return &usageCapture{
		spool:     spool,
		requestID: requestID,
		profileID: edge.ID,
		surface:   gatewayUsageSurface(surface),
		started:   time.Now(),
	}
}

func (capture *usageCapture) setModel(model string) {
	if capture == nil || model == "" {
		return
	}
	copy := model
	capture.model = &copy
}

func (capture *usageCapture) observeAttempt(member *PoolMember, model string) {
	if capture == nil || member == nil {
		return
	}
	// A capture represents the final client outcome. A pre-commit failover may
	// have observed an unusable upstream payload, so never carry its counters
	// into the member that ultimately serves (or fails) the request.
	capture.tokens = capturedGatewayUsage{}
	capture.sse = usageSSECapture{}
	capture.attempts++
	capture.ticketID = usageOptionalString(member.TicketID)
	capture.accountKind = usageOptionalString(member.SourceKind)
	capture.accountID = usageOptionalString(member.SourceID)
	// Go forwards the selected public model unchanged in every supported
	// Product transport. A configured single-model value is retained as the
	// source of truth; for a multi-model member the actual forwarded request
	// model is the only exact attribution available.
	capture.upstreamModel = usageOptionalString(member.UpstreamModel)
	if capture.upstreamModel == nil {
		capture.upstreamModel = usageOptionalString(model)
	}
	if channel := gatewayUsageChannel(member.UpstreamTransport); channel != "" {
		capture.upstreamChannel = &channel
	}
}

func (capture *usageCapture) markStreaming() {
	if capture != nil {
		capture.streaming = true
	}
}

func (capture *usageCapture) markFailure(class string) {
	if capture != nil {
		capture.failureClass = safeGatewayUsageErrorClass(class)
	}
}

// observeResponseJSON accepts only a complete JSON object with exact numeric
// usage fields. It is called after any protocol conversion, so a Responses
// request routed through Chat Completions is captured in the public Responses
// shape exactly as returned to the caller.
func (capture *usageCapture) observeResponseJSON(raw []byte) {
	if capture == nil {
		return
	}
	root, ok := decodeUsageJSONObject(raw)
	if !ok {
		return
	}
	if usage, ok := capturedUsageFromObject(root); ok {
		capture.tokens = usage
	}
}

// observeResponsesSSEEvent is the zero-copy hook used by the strict official
// Responses dispatchers. `event` has already passed their protocol validator;
// this method retains only terminal numeric usage.
func (capture *usageCapture) observeResponsesSSEEvent(kind string, event map[string]any) {
	if capture == nil || kind != "response.completed" || event == nil {
		return
	}
	if response, ok := event["response"].(map[string]any); ok {
		if usage, ok := capturedUsageFromObject(response); ok {
			capture.tokens = usage
		}
		return
	}
	if usage, ok := capturedUsageFromObject(event); ok {
		capture.tokens = usage
	}
}

// observeSSEChunk accepts raw SSE only while forwarding. It recognizes just
// the terminal usage carriers for the three supported public surfaces. Missing
// or malformed usage is deliberately left invalid/empty, which becomes zeros
// in the one final event.
func (capture *usageCapture) observeSSEChunk(chunk []byte) {
	if capture == nil || len(chunk) == 0 || capture.sse.invalid {
		return
	}
	if len(capture.sse.pending)+len(chunk) > maxUsageSSEFrameBytes {
		capture.sse.pending = nil
		capture.sse.invalid = true
		return
	}
	capture.sse.pending = append(capture.sse.pending, chunk...)
	for {
		end, width := usageSSEFrameEnd(capture.sse.pending)
		if end < 0 {
			return
		}
		frame := capture.sse.pending[:end]
		capture.sse.pending = capture.sse.pending[end+width:]
		capture.observeSSEFrame(frame)
		if capture.sse.invalid {
			capture.sse.pending = nil
			return
		}
	}
}

func (capture *usageCapture) observeSSEFrame(frame []byte) {
	event, data, hasData, ok := parseUsageSSEFrame(frame)
	if !ok {
		capture.sse.invalid = true
		return
	}
	if !hasData {
		return
	}
	if bytes.Equal(data, []byte("[DONE]")) {
		if capture.surface == "chat" && capture.sse.chat != nil {
			capture.tokens = *capture.sse.chat
		}
		return
	}
	root, ok := decodeUsageJSONObject(data)
	if !ok {
		// Only structured terminal/value-carrying frames participate in the
		// usage result. Ordinary extension frames can be ignored safely.
		return
	}
	switch capture.surface {
	case "messages":
		capture.observeMessagesSSE(event, root)
	case "responses":
		if event == "response.completed" {
			capture.observeResponsesSSEEvent(event, root)
		}
	case "chat":
		if _, exists := root["usage"]; exists {
			usage, valid := capturedUsageFromObject(root)
			if !valid {
				capture.sse.invalid = true
				return
			}
			capture.sse.chat = &usage
		}
	}
}

func (capture *usageCapture) observeMessagesSSE(event string, root map[string]any) {
	if capture == nil {
		return
	}
	switch event {
	case "message_start":
		message, ok := root["message"].(map[string]any)
		if !ok {
			capture.sse.invalid = true
			return
		}
		usage, exists := message["usage"]
		if !exists {
			capture.sse.invalid = true
			return
		}
		parsed, present, valid := capturedPartialUsage(usage, true, false)
		if !present || !valid || !capture.sse.setInput(parsed.input) || !capture.sse.setOptionalCached(parsed.cached) {
			capture.sse.invalid = true
		}
	case "message_delta":
		usage, exists := root["usage"]
		if !exists {
			capture.sse.invalid = true
			return
		}
		parsed, present, valid := capturedPartialUsage(usage, false, true)
		if !present || !valid || !capture.sse.setOutput(parsed.output) || !capture.sse.setOptionalReasoning(parsed.reasoning) {
			capture.sse.invalid = true
		}
	case "message_stop":
		if capture.sse.input == nil || capture.sse.output == nil {
			capture.sse.invalid = true
			return
		}
		capture.tokens = capturedGatewayUsage{
			input: *capture.sse.input, output: *capture.sse.output,
			cached: capture.sse.cached, reasoning: capture.sse.reasoning, valid: true,
		}
	}
}

func (capture *usageSSECapture) setInput(value uint64) bool {
	return capture.set(&capture.input, value)
}

func (capture *usageSSECapture) setOutput(value uint64) bool {
	return capture.set(&capture.output, value)
}

func (capture *usageSSECapture) setOptionalCached(value *uint64) bool {
	if value == nil {
		return true
	}
	return capture.set(&capture.cached, *value)
}

func (capture *usageSSECapture) setOptionalReasoning(value *uint64) bool {
	if value == nil {
		return true
	}
	return capture.set(&capture.reasoning, *value)
}

func (capture *usageSSECapture) set(target **uint64, value uint64) bool {
	if *target != nil && **target != value {
		return false
	}
	copy := value
	*target = &copy
	return true
}

func usageSSEFrameEnd(raw []byte) (int, int) {
	if index := bytes.Index(raw, []byte("\n\n")); index >= 0 {
		return index, 2
	}
	if index := bytes.Index(raw, []byte("\r\n\r\n")); index >= 0 {
		return index, 4
	}
	return -1, 0
}

func parseUsageSSEFrame(frame []byte) (string, []byte, bool, bool) {
	normalized := strings.ReplaceAll(strings.ReplaceAll(string(frame), "\r\n", "\n"), "\r", "\n")
	var event string
	var data []string
	for _, line := range strings.Split(normalized, "\n") {
		if line == "" || strings.HasPrefix(line, ":") {
			continue
		}
		field, value, found := strings.Cut(line, ":")
		if !found {
			return "", nil, false, false
		}
		value = strings.TrimPrefix(value, " ")
		switch field {
		case "event":
			if event != "" && event != value {
				return "", nil, false, false
			}
			event = value
		case "data":
			data = append(data, value)
		}
	}
	if len(data) == 0 {
		return event, nil, false, true
	}
	return event, []byte(strings.Join(data, "\n")), true, true
}

func decodeUsageJSONObject(raw []byte) (map[string]any, bool) {
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	var root map[string]any
	if err := decoder.Decode(&root); err != nil || root == nil {
		return nil, false
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return nil, false
	}
	return root, true
}

func capturedUsageFromObject(root map[string]any) (capturedGatewayUsage, bool) {
	if root == nil {
		return capturedGatewayUsage{}, false
	}
	usage, present, valid := capturedPartialUsage(root["usage"], true, true)
	return usage, present && valid
}

func capturedPartialUsage(raw any, needInput, needOutput bool) (capturedGatewayUsage, bool, bool) {
	usage, ok := raw.(map[string]any)
	if !ok || usage == nil {
		return capturedGatewayUsage{}, false, false
	}
	result := capturedGatewayUsage{}
	if needInput {
		input, present, valid := capturedUsageToken(usage, []string{"input_tokens", "prompt_tokens"}, nil)
		if !present || !valid {
			return capturedGatewayUsage{}, false, false
		}
		result.input = input
	}
	if needOutput {
		output, present, valid := capturedUsageToken(usage, []string{"output_tokens", "completion_tokens"}, nil)
		if !present || !valid {
			return capturedGatewayUsage{}, false, false
		}
		result.output = output
	}
	cached, cachedPresent, cachedValid := capturedUsageToken(usage, []string{"cached_input_tokens", "cache_read_input_tokens"}, [][2]string{{"input_tokens_details", "cached_tokens"}, {"prompt_tokens_details", "cached_tokens"}})
	if !cachedValid {
		return capturedGatewayUsage{}, false, false
	}
	if cachedPresent {
		result.cached = usageUintPointer(cached)
	}
	reasoning, reasoningPresent, reasoningValid := capturedUsageToken(usage, []string{"reasoning_tokens", "reasoning_output_tokens"}, [][2]string{{"output_tokens_details", "reasoning_tokens"}, {"completion_tokens_details", "reasoning_tokens"}})
	if !reasoningValid {
		return capturedGatewayUsage{}, false, false
	}
	if reasoningPresent {
		result.reasoning = usageUintPointer(reasoning)
	}
	result.valid = needInput && needOutput
	return result, true, true
}

// capturedUsageToken accepts equivalent numeric aliases only when every
// supplied value is a non-negative exact integer. It never derives a value
// from total_tokens or response text.
func capturedUsageToken(usage map[string]any, direct []string, nested [][2]string) (uint64, bool, bool) {
	var value uint64
	present := false
	observe := func(raw any) bool {
		parsed, ok := capturedUsageUint(raw)
		if !ok {
			return false
		}
		if present && value != parsed {
			return false
		}
		value = parsed
		present = true
		return true
	}
	for _, key := range direct {
		if raw, exists := usage[key]; exists && !observe(raw) {
			return 0, false, false
		}
	}
	for _, path := range nested {
		container, exists := usage[path[0]]
		if !exists {
			continue
		}
		object, ok := container.(map[string]any)
		if !ok {
			return 0, false, false
		}
		if raw, exists := object[path[1]]; exists && !observe(raw) {
			return 0, false, false
		}
	}
	return value, present, true
}

func capturedUsageUint(raw any) (uint64, bool) {
	switch value := raw.(type) {
	case json.Number:
		if strings.ContainsAny(value.String(), ".eE+-") {
			return 0, false
		}
		parsed, err := value.Int64()
		return uint64(parsed), err == nil && parsed >= 0
	case uint64:
		return value, true
	case uint:
		return uint64(value), true
	case uint32:
		return uint64(value), true
	case int:
		return uint64(value), value >= 0
	case int64:
		return uint64(value), value >= 0
	case float64:
		if value < 0 || value > (1<<53)-1 || value != float64(uint64(value)) {
			return 0, false
		}
		return uint64(value), true
	default:
		return 0, false
	}
}

func usageUintPointer(value uint64) *uint64 {
	copy := value
	return &copy
}

func (capture *usageCapture) finish(rt *Runtime, observed *edgeStatusResponseWriter, requestCanceled bool) {
	if capture == nil || observed == nil {
		return
	}
	defer func() { capture.sse.pending = nil }()
	completed := time.Now().UTC()
	statusCode := observed.statusCode()
	writeFailed := observed.downstreamWriteFailed()
	succeeded := statusCode >= http.StatusOK && statusCode < http.StatusMultipleChoices && !requestCanceled && !writeFailed && capture.failureClass == ""
	var statusCodeValue *uint16
	if statusCode > 0 && statusCode <= int(^uint16(0)) {
		value := uint16(statusCode)
		statusCodeValue = &value
	}
	latency := uint64(time.Since(capture.started).Milliseconds())
	event := gatewayUsageEvent{
		RequestID:       capture.requestID,
		Timestamp:       completed.Format(time.RFC3339Nano),
		ProfileID:       capture.profileID,
		Surface:         capture.surface,
		UpstreamChannel: capture.upstreamChannel,
		TicketID:        capture.ticketID,
		AccountKind:     capture.accountKind,
		AccountID:       capture.accountID,
		Model:           capture.model,
		UpstreamModel:   capture.upstreamModel,
		InputTokens:     0,
		OutputTokens:    0,
		Status:          "failed",
		StatusCode:      statusCodeValue,
		LatencyMS:       &latency,
	}
	if capture.attempts > 0 {
		attempts := capture.attempts
		event.Attempts = &attempts
	}
	if capture.streaming {
		if firstWrite := observed.firstWriteAt(); !firstWrite.IsZero() {
			ttft := uint64(firstWrite.Sub(capture.started).Milliseconds())
			if firstWrite.Before(capture.started) {
				ttft = 0
			}
			event.TTFTMS = &ttft
		}
	}
	if succeeded {
		event.Status = "ok"
		if capture.tokens.valid {
			event.InputTokens = capture.tokens.input
			event.OutputTokens = capture.tokens.output
			event.CachedInputTokens = capture.tokens.cached
			event.ReasoningTokens = capture.tokens.reasoning
		}
	} else {
		errorClass := capture.failureClass
		if errorClass == "" {
			errorClass = gatewayUsageErrorClass(statusCode, requestCanceled, writeFailed)
		}
		event.ErrorClass = &errorClass
	}
	if !capture.spool.record(event) {
		rt.logf("usage capture dropped code=usage_spool_write_failed")
	}
}

func usageOptionalString(value string) *string {
	if value == "" {
		return nil
	}
	copy := value
	return &copy
}

func newGatewayUsageUUIDv4() (string, bool) {
	var raw [16]byte
	if _, err := rand.Read(raw[:]); err != nil {
		return "", false
	}
	raw[6] = (raw[6] & 0x0f) | 0x40
	raw[8] = (raw[8] & 0x3f) | 0x80
	encoded := hex.EncodeToString(raw[:])
	return encoded[0:8] + "-" + encoded[8:12] + "-" + encoded[12:16] + "-" + encoded[16:20] + "-" + encoded[20:32], true
}

func gatewayUsageSurface(surface string) string {
	switch surface {
	case surfaceResponses:
		return "responses"
	case surfaceMessages:
		return "messages"
	case surfaceChatCompletions:
		return "chat"
	default:
		return "messages"
	}
}

func gatewayUsageChannel(transport string) string {
	switch transport {
	case transportAnthropicMessages:
		return "anthropic"
	case transportCodexResponses:
		return "codex_responses"
	case transportGrokResponses:
		return "grok"
	case transportOpenAIChatCompletions:
		return "openai_chat"
	default:
		return ""
	}
}

func gatewayUsageErrorClass(statusCode int, requestCanceled, downstreamWriteFailed bool) string {
	if requestCanceled {
		return edgeStatusRequestCanceled
	}
	if downstreamWriteFailed {
		return edgeStatusDownstreamWrite
	}
	return edgeRequestStatusErrorCode(statusCode, false, false)
}

func safeGatewayUsageErrorClass(class string) string {
	switch class {
	case edgeStatusRouteBusy,
		edgeStatusRequestCanceled,
		edgeStatusDownstreamWrite,
		edgeStatusInvalidRequest,
		edgeStatusRequestUnauthorized,
		edgeStatusRequestNotFound,
		edgeStatusRequestTooLarge,
		edgeStatusUpstreamUnavailable,
		edgeStatusRequestFailed:
		return class
	default:
		return edgeStatusRequestFailed
	}
}
