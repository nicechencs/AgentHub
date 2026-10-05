package main

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// gatewayUsageEvent is intentionally the same JSONL wire shape consumed by
// core's GatewayUsageEvent. Keep this narrow: source identities, tickets,
// sessions, URLs, request/response payloads, and login material do not belong
// in the sidecar spool.
type gatewayUsageEvent struct {
	RequestID       string  `json:"request_id"`
	Timestamp       string  `json:"ts"`
	ProfileID       string  `json:"profile_id"`
	Surface         string  `json:"surface"`
	UpstreamChannel *string `json:"upstream_channel,omitempty"`
	TicketID        *string `json:"ticket_id,omitempty"`
	AccountKind     *string `json:"account_source_kind,omitempty"`
	AccountID       *string `json:"account_source_id,omitempty"`
	Model           *string `json:"model,omitempty"`
	UpstreamModel   *string `json:"upstream_model,omitempty"`
	InputTokens     uint64  `json:"input_tokens"`
	OutputTokens    uint64  `json:"output_tokens"`
	Status          string  `json:"status"`
	StatusCode      *uint16 `json:"status_code,omitempty"`
	ErrorClass      *string `json:"error_class,omitempty"`
	LatencyMS       *uint64 `json:"latency_ms,omitempty"`
	TTFTMS          *uint64 `json:"ttft_ms,omitempty"`
	Attempts        *uint32 `json:"attempts,omitempty"`
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
}

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

func (capture *usageCapture) finish(rt *Runtime, observed *edgeStatusResponseWriter, requestCanceled bool) {
	if capture == nil || observed == nil {
		return
	}
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
