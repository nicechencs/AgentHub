package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"mime"
	"net"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// routeHTTPSafetyPolicy keeps every HTTP resource boundary in one place. Tests
// may copy this value and shorten timeouts, but production always uses the
// complete default policy below.
type routeHTTPSafetyPolicy struct {
	IngressBodyBytes        int64
	ControlBodyBytes        int64
	NonStreamBodyBytes      int64
	NonStreamIdleTimeout    time.Duration
	NonStreamTotalTimeout   time.Duration
	SSEBodyBytes            int64
	SSEIdleTimeout          time.Duration
	MaxConcurrentRequests   int
	MaxHeaderBytes          int
	ServerReadHeaderTimeout time.Duration
	ServerReadTimeout       time.Duration
	ServerIdleTimeout       time.Duration
	DownstreamWriteTimeout  time.Duration
	ControlWriteTimeout     time.Duration
	UpstreamDialTimeout     time.Duration
	UpstreamTLSHandshake    time.Duration
	UpstreamHeaderTimeout   time.Duration
	UpstreamIdleTimeout     time.Duration
	UpstreamMaxConns        int
}

var defaultRouteHTTPSafetyPolicy = routeHTTPSafetyPolicy{
	IngressBodyBytes:        8 << 20,
	ControlBodyBytes:        1 << 20,
	NonStreamBodyBytes:      32 << 20,
	NonStreamIdleTimeout:    30 * time.Second,
	NonStreamTotalTimeout:   2 * time.Minute,
	SSEBodyBytes:            32 << 20,
	SSEIdleTimeout:          30 * time.Second,
	MaxConcurrentRequests:   16,
	MaxHeaderBytes:          64 << 10,
	ServerReadHeaderTimeout: 10 * time.Second,
	ServerReadTimeout:       30 * time.Second,
	ServerIdleTimeout:       90 * time.Second,
	DownstreamWriteTimeout:  30 * time.Second,
	ControlWriteTimeout:     10 * time.Second,
	UpstreamDialTimeout:     10 * time.Second,
	UpstreamTLSHandshake:    10 * time.Second,
	UpstreamHeaderTimeout:   90 * time.Second,
	UpstreamIdleTimeout:     90 * time.Second,
	UpstreamMaxConns:        16,
}

var (
	errBodyTooLarge           = errors.New("request body is too large")
	errUpstreamBodyTooLarge   = errors.New("upstream response body is too large")
	errUpstreamBodyIdle       = errors.New("upstream response body was idle too long")
	errUpstreamBodyTimeout    = errors.New("upstream response body exceeded its total time limit")
	errUnexpectedResponseType = errors.New("upstream response content type is invalid")
	errSSEIdle                = errors.New("upstream SSE response was idle too long")
	errSSEEmpty               = errors.New("upstream SSE response ended before any data")
	errDownstreamWrite        = errors.New("downstream response write failed")
	sharedUpstreamHTTPClient  = newUpstreamHTTPClientWithPolicy(defaultRouteHTTPSafetyPolicy)
)

func readStrictRequestBody(w http.ResponseWriter, r *http.Request, max int64) ([]byte, error) {
	if max <= 0 || (r.ContentLength >= 0 && r.ContentLength > max) {
		return nil, errBodyTooLarge
	}
	limited := http.MaxBytesReader(w, r.Body, max)
	raw, err := io.ReadAll(limited)
	if err != nil {
		var maxErr *http.MaxBytesError
		if errors.As(err, &maxErr) {
			return nil, errBodyTooLarge
		}
		return nil, err
	}
	return raw, nil
}

func readBoundedResponseBody(ctx context.Context, resp *http.Response, max int64, idleTimeout, totalTimeout time.Duration) ([]byte, error) {
	if resp == nil || resp.Body == nil {
		return nil, io.ErrUnexpectedEOF
	}
	if max <= 0 || resp.ContentLength > max {
		return nil, errUpstreamBodyTooLarge
	}
	readCtx, cancel := context.WithCancel(ctx)
	defer cancel()
	chunks := make(chan boundedStreamChunk, 1)
	go func() {
		defer close(chunks)
		buf := make([]byte, 32*1024)
		for {
			n, err := resp.Body.Read(buf)
			if n > 0 {
				chunk := append([]byte(nil), buf[:n]...)
				select {
				case chunks <- boundedStreamChunk{data: chunk}:
				case <-readCtx.Done():
					return
				}
			}
			if err != nil {
				select {
				case chunks <- boundedStreamChunk{err: err}:
				case <-readCtx.Done():
				}
				return
			}
		}
	}()

	var idle *time.Timer
	var idleC <-chan time.Time
	if idleTimeout > 0 {
		idle = time.NewTimer(idleTimeout)
		idleC = idle.C
		defer idle.Stop()
	}
	var total *time.Timer
	var totalC <-chan time.Time
	if totalTimeout > 0 {
		total = time.NewTimer(totalTimeout)
		totalC = total.C
		defer total.Stop()
	}
	started := time.Now()
	idleDeadline := started.Add(idleTimeout)
	totalDeadline := started.Add(totalTimeout)
	raw := make([]byte, 0, minInt64(resp.ContentLength, max))
	for {
		now := time.Now()
		if totalTimeout > 0 && !now.Before(totalDeadline) {
			_ = resp.Body.Close()
			return nil, errUpstreamBodyTimeout
		}
		if idleTimeout > 0 && !now.Before(idleDeadline) {
			_ = resp.Body.Close()
			return nil, errUpstreamBodyIdle
		}
		select {
		case <-ctx.Done():
			_ = resp.Body.Close()
			return nil, ctx.Err()
		case <-idleC:
			_ = resp.Body.Close()
			return nil, errUpstreamBodyIdle
		case <-totalC:
			_ = resp.Body.Close()
			return nil, errUpstreamBodyTimeout
		case item, ok := <-chunks:
			if !ok {
				return nil, io.ErrUnexpectedEOF
			}
			if len(item.data) > 0 {
				now = time.Now()
				if totalTimeout > 0 && !now.Before(totalDeadline) {
					_ = resp.Body.Close()
					return nil, errUpstreamBodyTimeout
				}
				if idleTimeout > 0 && !now.Before(idleDeadline) {
					_ = resp.Body.Close()
					return nil, errUpstreamBodyIdle
				}
				if idle != nil {
					if !idle.Stop() {
						select {
						case <-idle.C:
						default:
						}
					}
					idle.Reset(idleTimeout)
					idleDeadline = now.Add(idleTimeout)
				}
				if int64(len(raw))+int64(len(item.data)) > max {
					_ = resp.Body.Close()
					return nil, errUpstreamBodyTooLarge
				}
				raw = append(raw, item.data...)
			}
			if item.err != nil {
				if errors.Is(item.err, io.EOF) {
					return raw, nil
				}
				return nil, item.err
			}
		}
	}
}

func minInt64(value, max int64) int {
	if value <= 0 || value > max {
		return 0
	}
	return int(value)
}

func newUpstreamHTTPClientWithPolicy(policy routeHTTPSafetyPolicy) *http.Client {
	dialer := &net.Dialer{Timeout: policy.UpstreamDialTimeout, KeepAlive: 30 * time.Second}
	return newUpstreamHTTPClientWithNetwork(policy, net.DefaultResolver, dialer)
}

func newUpstreamHTTPClientWithNetwork(policy routeHTTPSafetyPolicy, resolver upstreamIPResolver, dialer upstreamContextDialer) *http.Client {
	transport := &http.Transport{
		Proxy:                  nil,
		DialContext:            validatedUpstreamDialContext(resolver, dialer),
		ForceAttemptHTTP2:      true,
		MaxIdleConns:           policy.UpstreamMaxConns,
		MaxIdleConnsPerHost:    policy.UpstreamMaxConns,
		MaxConnsPerHost:        policy.UpstreamMaxConns,
		IdleConnTimeout:        policy.UpstreamIdleTimeout,
		TLSHandshakeTimeout:    policy.UpstreamTLSHandshake,
		ResponseHeaderTimeout:  policy.UpstreamHeaderTimeout,
		ExpectContinueTimeout:  time.Second,
		MaxResponseHeaderBytes: int64(policy.MaxHeaderBytes),
	}
	return &http.Client{
		Transport: transport,
		CheckRedirect: func(_ *http.Request, _ []*http.Request) error {
			return http.ErrUseLastResponse
		},
	}
}

func newUpstreamHTTPClient() *http.Client {
	return sharedUpstreamHTTPClient
}

func isJSONMediaType(raw string) bool {
	mediaType, _, err := mime.ParseMediaType(raw)
	if err != nil {
		return false
	}
	mediaType = strings.ToLower(mediaType)
	return mediaType == "application/json" || strings.HasSuffix(mediaType, "+json")
}

func isJSONObject(raw []byte) bool {
	var object map[string]json.RawMessage
	return json.Unmarshal(raw, &object) == nil && object != nil
}

func isEventStreamMediaType(raw string) bool {
	mediaType, _, err := mime.ParseMediaType(raw)
	return err == nil && strings.EqualFold(mediaType, "text/event-stream")
}

func safeRetryAfter(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	seconds, err := strconv.ParseInt(raw, 10, 64)
	if err != nil || seconds < 0 {
		return ""
	}
	if seconds > int64(maxCooldown/time.Second) {
		seconds = int64(maxCooldown / time.Second)
	}
	return strconv.FormatInt(seconds, 10)
}

func setSafeErrorHeaders(dst http.Header, source http.Header) {
	dst.Set("Content-Type", "application/json")
	if retryAfter := safeRetryAfter(source.Get("Retry-After")); retryAfter != "" {
		dst.Set("Retry-After", retryAfter)
	}
}

func setSafeSuccessHeaders(dst http.Header, stream bool) {
	if stream {
		dst.Set("Content-Type", "text/event-stream")
		dst.Set("Cache-Control", "no-cache")
		return
	}
	dst.Set("Content-Type", "application/json")
}

func refreshDownstreamWriteDeadline(w http.ResponseWriter, timeout time.Duration) {
	if timeout <= 0 {
		return
	}
	_ = http.NewResponseController(w).SetWriteDeadline(time.Now().Add(timeout))
}

func safeUpstreamErrorBody() []byte {
	return []byte(`{"error":{"code":"upstream_error","message":"The upstream response could not be used.","type":"api_error"}}`)
}

func writeSafeUpstreamResponse(w http.ResponseWriter, status int, source http.Header) {
	refreshDownstreamWriteDeadline(w, defaultRouteHTTPSafetyPolicy.DownstreamWriteTimeout)
	setSafeErrorHeaders(w.Header(), source)
	w.WriteHeader(status)
	_, _ = w.Write(safeUpstreamErrorBody())
}

func writeSafeSSETermination(w http.ResponseWriter) {
	refreshDownstreamWriteDeadline(w, defaultRouteHTTPSafetyPolicy.DownstreamWriteTimeout)
	_, _ = io.WriteString(w, "event: error\ndata: {\"error\":{\"code\":\"upstream_error\",\"message\":\"The upstream stream ended unexpectedly.\",\"type\":\"api_error\"}}\n\n")
	if flusher, ok := w.(http.Flusher); ok {
		flusher.Flush()
	}
}

type boundedStreamChunk struct {
	data []byte
	err  error
}

// observe receives transient upstream bytes before they are forwarded. Callers
// use it only for bounded, numeric usage extraction; it must not retain or log
// payload content.
func relayBoundedSSE(w http.ResponseWriter, r *http.Request, resp *http.Response, policy routeHTTPSafetyPolicy, observe func([]byte)) (bool, error) {
	if !isEventStreamMediaType(resp.Header.Get("Content-Type")) {
		return false, errUnexpectedResponseType
	}
	streamCtx, cancel := context.WithCancel(r.Context())
	defer cancel()
	chunks := make(chan boundedStreamChunk, 1)
	go func() {
		defer close(chunks)
		buf := make([]byte, 4096)
		for {
			n, err := resp.Body.Read(buf)
			if n > 0 {
				chunk := append([]byte(nil), buf[:n]...)
				select {
				case chunks <- boundedStreamChunk{data: chunk}:
				case <-streamCtx.Done():
					return
				}
			}
			if err != nil {
				select {
				case chunks <- boundedStreamChunk{err: err}:
				case <-streamCtx.Done():
				}
				return
			}
		}
	}()

	idle := time.NewTimer(policy.SSEIdleTimeout)
	defer idle.Stop()
	committed := false
	var total int64
	for {
		select {
		case <-r.Context().Done():
			_ = resp.Body.Close()
			return committed, r.Context().Err()
		case <-idle.C:
			_ = resp.Body.Close()
			return committed, errSSEIdle
		case item, ok := <-chunks:
			if !ok {
				if committed {
					return true, nil
				}
				return false, errSSEEmpty
			}
			if item.err != nil {
				if errors.Is(item.err, io.EOF) && committed {
					return true, nil
				}
				if errors.Is(item.err, io.EOF) {
					return false, errSSEEmpty
				}
				return committed, item.err
			}
			if !idle.Stop() {
				select {
				case <-idle.C:
				default:
				}
			}
			idle.Reset(policy.SSEIdleTimeout)
			total += int64(len(item.data))
			if total > policy.SSEBodyBytes {
				_ = resp.Body.Close()
				return committed, errUpstreamBodyTooLarge
			}
			if !committed {
				committed = true
				refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
				setSafeSuccessHeaders(w.Header(), true)
				w.WriteHeader(resp.StatusCode)
			}
			if observe != nil {
				observe(item.data)
			}
			refreshDownstreamWriteDeadline(w, policy.DownstreamWriteTimeout)
			if _, err := w.Write(item.data); err != nil {
				_ = resp.Body.Close()
				return committed, fmt.Errorf("%w: %v", errDownstreamWrite, err)
			}
			if flusher, ok := w.(http.Flusher); ok {
				flusher.Flush()
			}
		}
	}
}
