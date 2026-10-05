package main

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strconv"
	"strings"
	"time"
)

type cappedCapture struct {
	buf      bytes.Buffer
	limit    int
	overflow bool
}

type officialSSEEventSanitizer func([]byte) ([]byte, error)

type officialStreamAffinity struct {
	ResponseID string
	Completed  bool
}

func newCappedCapture(limit int) *cappedCapture { return &cappedCapture{limit: limit} }

func (capture *cappedCapture) Write(raw []byte) (int, error) {
	original := len(raw)
	remaining := capture.limit - capture.buf.Len()
	if remaining < len(raw) {
		capture.overflow = true
		if remaining < 0 {
			remaining = 0
		}
		raw = raw[:remaining]
	}
	_, _ = capture.buf.Write(raw)
	return original, nil
}

func (capture *cappedCapture) Bytes() []byte    { return capture.buf.Bytes() }
func (capture *cappedCapture) Overflowed() bool { return capture.overflow }

func dispatchOfficialResponsesStream(
	w http.ResponseWriter,
	r *http.Request,
	pool *Pool,
	member *PoolMember,
	model string,
	resp *http.Response,
	excluded *[]string,
	lastStatus *int,
	lastHeader *http.Header,
	lastBody *[]byte,
	hasLast *bool,
	policy routeHTTPSafetyPolicy,
	sanitize officialSSEEventSanitizer,
	affinity *officialStreamAffinity,
	usage *usageCapture,
	streamFailed *bool,
) int {
	defer resp.Body.Close()
	if !isEventStreamMediaType(resp.Header.Get("Content-Type")) {
		pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = http.StatusBadGateway
		*lastHeader = make(http.Header)
		*lastBody = safeUpstreamErrorBody()
		return dispatchContinue
	}

	var observed officialStreamAffinity
	committed, safeSequence, relayErr := relayStrictResponsesSSE(w, r, resp, policy, sanitize, &observed, usage)
	if relayErr == nil {
		if affinity != nil {
			*affinity = observed
		}
		pool.ReportSuccess(member.ID)
		return dispatchDone
	}
	if r.Context().Err() != nil || errors.Is(relayErr, errDownstreamWrite) {
		return dispatchDone
	}
	if errors.Is(relayErr, errOfficialResponsesFailure) {
		if streamFailed != nil {
			*streamFailed = true
		}
		pool.ReportFailure(member.ID, model, classRequest, 0, time.Now())
		if !committed {
			refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
			setSafeSuccessHeaders(w.Header(), true)
			w.WriteHeader(http.StatusOK)
		}
		writeSafeResponsesSSETermination(w, safeSequence, policy.DownstreamWriteTimeout)
		return dispatchDone
	}
	pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
	if committed {
		if streamFailed != nil {
			*streamFailed = true
		}
		writeSafeResponsesSSETermination(w, safeSequence, policy.DownstreamWriteTimeout)
		return dispatchDone
	}
	*excluded = append(*excluded, member.ID)
	*hasLast = true
	*lastStatus = http.StatusBadGateway
	*lastHeader = make(http.Header)
	*lastBody = safeUpstreamErrorBody()
	return dispatchContinue
}

func relayStrictResponsesSSE(w http.ResponseWriter, r *http.Request, resp *http.Response, policy routeHTTPSafetyPolicy, sanitize officialSSEEventSanitizer, affinity *officialStreamAffinity, usage *usageCapture) (bool, uint64, error) {
	streamCtx, cancel := context.WithCancel(r.Context())
	defer cancel()
	lines := scanConvertedSSELines(streamCtx, resp.Body, policy.SSEBodyBytes)
	idle := time.NewTimer(policy.SSEIdleTimeout)
	defer idle.Stop()
	state := newResponsesSSEState()
	frameLines := make([]string, 0, 4)
	committed := false
	var safeSequence uint64
	var total int64

	consume := func() error {
		if len(frameLines) == 0 {
			return nil
		}
		frame := []byte(strings.Join(frameLines, "\n"))
		frameLines = frameLines[:0]
		event, err := state.consumeFrame(frame)
		if err != nil {
			return err
		}
		if event == nil {
			return nil
		}
		if event.ErrorLike {
			safeSequence = event.SequenceNumber
			return errOfficialResponsesFailure
		}
		observeOfficialStreamAffinity(affinity, event.Type, event.Value)
		usage.observeResponsesSSEEvent(event.Type, event.Value)
		safeSequence = event.SequenceNumber + 1
		if sanitize != nil {
			_, payload, hasData, parseErr := parseResponsesSSEFields(frame)
			if parseErr != nil || !hasData {
				return errInvalidResponsesSSE
			}
			sanitized, sanitizeErr := sanitize(payload)
			if sanitizeErr != nil {
				return errInvalidResponsesSSE
			}
			frame = []byte("event: " + event.Type + "\ndata: " + string(sanitized))
		}
		if !committed {
			committed = true
			refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
			setSafeSuccessHeaders(w.Header(), true)
			w.WriteHeader(http.StatusOK)
		}
		refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
		if _, err := w.Write(append(frame, '\n', '\n')); err != nil {
			return errors.Join(errDownstreamWrite, err)
		}
		if flusher, ok := w.(http.Flusher); ok {
			flusher.Flush()
		}
		return nil
	}

	for {
		select {
		case <-r.Context().Done():
			_ = resp.Body.Close()
			return committed, safeSequence, r.Context().Err()
		case <-idle.C:
			_ = resp.Body.Close()
			return committed, safeSequence, errSSEIdle
		case item, ok := <-lines:
			if !ok {
				return committed, safeSequence, errInvalidResponsesSSE
			}
			if item.err != nil {
				if errors.Is(item.err, io.EOF) && len(frameLines) == 0 {
					return committed, safeSequence, state.finish()
				}
				return committed, safeSequence, errInvalidResponsesSSE
			}
			if !idle.Stop() {
				select {
				case <-idle.C:
				default:
				}
			}
			idle.Reset(policy.SSEIdleTimeout)
			total += int64(len(item.line) + 1)
			if total > policy.SSEBodyBytes {
				_ = resp.Body.Close()
				return committed, safeSequence, errUpstreamBodyTooLarge
			}
			line := strings.TrimSuffix(item.line, "\r")
			if line == "" {
				if err := consume(); err != nil {
					return committed, safeSequence, err
				}
				continue
			}
			frameLines = append(frameLines, line)
		}
	}
}

func writeSafeResponsesSSETermination(w http.ResponseWriter, sequence uint64, timeout time.Duration) {
	refreshDownstreamWriteDeadline(w, timeout)
	_, _ = io.WriteString(w, "event: error\ndata: {\"type\":\"error\",\"sequence_number\":"+
		strconv.FormatUint(sequence, 10)+",\"error\":{\"code\":\"upstream_error\",\"message\":\"The upstream stream ended unexpectedly.\",\"type\":\"api_error\"}}\n\n")
	if flusher, ok := w.(http.Flusher); ok {
		flusher.Flush()
	}
}

func dispatchGrokResponsesStream(
	w http.ResponseWriter,
	r *http.Request,
	pool *Pool,
	member *PoolMember,
	model string,
	resp *http.Response,
	excluded *[]string,
	lastStatus *int,
	lastHeader *http.Header,
	lastBody *[]byte,
	hasLast *bool,
	policy routeHTTPSafetyPolicy,
	sanitize officialSSEEventSanitizer,
	affinity *officialStreamAffinity,
	usage *usageCapture,
	streamFailed *bool,
) int {
	defer resp.Body.Close()
	if !isEventStreamMediaType(resp.Header.Get("Content-Type")) {
		pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
		*excluded = append(*excluded, member.ID)
		*hasLast = true
		*lastStatus = http.StatusBadGateway
		*lastHeader = make(http.Header)
		*lastBody = safeUpstreamErrorBody()
		return dispatchContinue
	}
	var observed officialStreamAffinity
	committed, safeSequence, relayErr := relayGrokResponsesSSE(w, r, resp, policy, sanitize, &observed, usage)
	if relayErr == nil {
		if affinity != nil {
			*affinity = observed
		}
		pool.ReportSuccess(member.ID)
		return dispatchDone
	}
	if r.Context().Err() != nil || errors.Is(relayErr, errDownstreamWrite) {
		return dispatchDone
	}
	if errors.Is(relayErr, errOfficialResponsesFailure) {
		if streamFailed != nil {
			*streamFailed = true
		}
		pool.ReportFailure(member.ID, model, classRequest, 0, time.Now())
		if !committed {
			refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
			setSafeSuccessHeaders(w.Header(), true)
			w.WriteHeader(http.StatusOK)
		}
		writeSafeGrokPairSSETermination(w, safeSequence, policy.DownstreamWriteTimeout, sanitize != nil)
		return dispatchDone
	}
	pool.ReportFailure(member.ID, model, classTransient, 0, time.Now())
	if committed {
		if streamFailed != nil {
			*streamFailed = true
		}
		writeSafeGrokPairSSETermination(w, safeSequence, policy.DownstreamWriteTimeout, sanitize != nil)
		return dispatchDone
	}
	*excluded = append(*excluded, member.ID)
	*hasLast = true
	*lastStatus = http.StatusBadGateway
	*lastHeader = make(http.Header)
	*lastBody = safeUpstreamErrorBody()
	return dispatchContinue
}

func relayGrokResponsesSSE(w http.ResponseWriter, r *http.Request, resp *http.Response, policy routeHTTPSafetyPolicy, sanitize officialSSEEventSanitizer, affinity *officialStreamAffinity, usage *usageCapture) (bool, uint64, error) {
	streamCtx, cancel := context.WithCancel(r.Context())
	defer cancel()
	lines := scanConvertedSSELines(streamCtx, resp.Body, policy.SSEBodyBytes)
	idle := time.NewTimer(policy.SSEIdleTimeout)
	defer idle.Stop()
	frameLines := make([]string, 0, 4)
	committed := false
	terminal := false
	var safeSequence uint64
	var total int64

	consume := func() error {
		if len(frameLines) == 0 {
			return nil
		}
		frame := []byte(strings.Join(frameLines, "\n"))
		frameLines = frameLines[:0]
		kind, hasData, err := parseGrokResponsesFrame(frame)
		if err != nil {
			return errInvalidResponsesSSE
		}
		if !hasData {
			return nil
		}
		if terminal {
			return errInvalidResponsesSSE
		}
		if kind == "response.failed" || kind == "error" {
			terminal = true
			return errOfficialResponsesFailure
		}
		if isResponsesTerminalEvent(kind) {
			terminal = true
		}
		payload, payloadErr := grokResponsesFramePayload(frame)
		if payloadErr != nil {
			return errInvalidResponsesSSE
		}
		var eventValue map[string]any
		if json.Unmarshal(payload, &eventValue) != nil || eventValue == nil {
			return errInvalidResponsesSSE
		}
		observeOfficialStreamAffinity(affinity, kind, eventValue)
		usage.observeResponsesSSEEvent(kind, eventValue)
		if sanitize != nil {
			sanitized, sanitizeErr := sanitize(payload)
			if sanitizeErr != nil {
				return errInvalidResponsesSSE
			}
			var value map[string]any
			if json.Unmarshal(sanitized, &value) != nil || value == nil {
				return errInvalidResponsesSSE
			}
			value["sequence_number"] = safeSequence
			sanitized, sanitizeErr = json.Marshal(value)
			if sanitizeErr != nil {
				return errInvalidResponsesSSE
			}
			frame = []byte("event: " + kind + "\ndata: " + string(sanitized))
			safeSequence++
		}
		if !committed {
			committed = true
			refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
			setSafeSuccessHeaders(w.Header(), true)
			w.WriteHeader(http.StatusOK)
		}
		refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
		if _, err := w.Write(append(frame, '\n', '\n')); err != nil {
			return errors.Join(errDownstreamWrite, err)
		}
		if flusher, ok := w.(http.Flusher); ok {
			flusher.Flush()
		}
		return nil
	}

	for {
		select {
		case <-r.Context().Done():
			_ = resp.Body.Close()
			return committed, safeSequence, r.Context().Err()
		case <-idle.C:
			_ = resp.Body.Close()
			return committed, safeSequence, errSSEIdle
		case item, ok := <-lines:
			if !ok {
				return committed, safeSequence, errInvalidResponsesSSE
			}
			if item.err != nil {
				if errors.Is(item.err, io.EOF) && len(frameLines) == 0 && terminal {
					return committed, safeSequence, nil
				}
				return committed, safeSequence, errInvalidResponsesSSE
			}
			if !idle.Stop() {
				select {
				case <-idle.C:
				default:
				}
			}
			idle.Reset(policy.SSEIdleTimeout)
			total += int64(len(item.line) + 1)
			if total > policy.SSEBodyBytes {
				_ = resp.Body.Close()
				return committed, safeSequence, errUpstreamBodyTooLarge
			}
			line := strings.TrimSuffix(item.line, "\r")
			if line == "" {
				if err := consume(); err != nil {
					return committed, safeSequence, err
				}
			} else {
				frameLines = append(frameLines, line)
			}
		}
	}
}

func observeOfficialStreamAffinity(affinity *officialStreamAffinity, kind string, event map[string]any) {
	if affinity == nil {
		return
	}
	if response, ok := event["response"].(map[string]any); ok {
		if responseID := grokOfficialResponseID(response); responseID != "" {
			affinity.ResponseID = responseID
		}
	}
	if isResponsesTerminalEvent(kind) {
		affinity.Completed = kind == "response.completed"
		if !affinity.Completed {
			affinity.ResponseID = ""
		}
	}
}

func writeSafeGrokPairSSETermination(w http.ResponseWriter, sequence uint64, timeout time.Duration, codexShape bool) {
	if codexShape {
		writeSafeResponsesSSETermination(w, sequence, timeout)
		return
	}
	writeSafeGrokSSETermination(w, timeout)
}

func parseGrokResponsesFrame(frame []byte) (string, bool, error) {
	normalized := strings.ReplaceAll(strings.ReplaceAll(string(frame), "\r\n", "\n"), "\r", "\n")
	var eventName string
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
			if eventName != "" {
				return "", false, errInvalidResponsesSSE
			}
			eventName = value
		case "data":
			dataLines = append(dataLines, value)
		}
	}
	if len(dataLines) == 0 {
		return "", false, nil
	}
	payload := []byte(strings.Join(dataLines, "\n"))
	decoder := json.NewDecoder(bytes.NewReader(payload))
	decoder.UseNumber()
	var value map[string]any
	if decoder.Decode(&value) != nil || value == nil {
		return "", false, errInvalidResponsesSSE
	}
	var trailing any
	if decoder.Decode(&trailing) != io.EOF {
		return "", false, errInvalidResponsesSSE
	}
	kind, ok := value["type"].(string)
	if !ok || !isKnownResponsesEventType(kind) || (eventName != "" && eventName != kind) {
		return "", false, errInvalidResponsesSSE
	}
	return kind, true, nil
}

func grokResponsesFramePayload(frame []byte) ([]byte, error) {
	normalized := strings.ReplaceAll(strings.ReplaceAll(string(frame), "\r\n", "\n"), "\r", "\n")
	var dataLines []string
	for _, line := range strings.Split(normalized, "\n") {
		if value, ok := strings.CutPrefix(line, "data:"); ok {
			dataLines = append(dataLines, strings.TrimSpace(value))
		}
	}
	if len(dataLines) == 0 {
		return nil, errInvalidResponsesSSE
	}
	return []byte(strings.Join(dataLines, "\n")), nil
}

func writeSafeGrokSSETermination(w http.ResponseWriter, timeout time.Duration) {
	refreshDownstreamWriteDeadline(w, timeout)
	_, _ = io.WriteString(w, "event: error\ndata: {\"type\":\"error\",\"error\":{\"code\":\"upstream_error\",\"message\":\"The upstream stream ended unexpectedly.\",\"type\":\"api_error\"}}\n\n")
	if flusher, ok := w.(http.Flusher); ok {
		flusher.Flush()
	}
}
