package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"time"
)

func (rt *Runtime) messagesMux() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/v1/messages", rt.handleMessages)
	return mux
}

func (rt *Runtime) handleMessages(w http.ResponseWriter, r *http.Request) {
	if r.RemoteAddr != "" && !isLoopbackRemote(r.RemoteAddr) {
		http.Error(w, "loopback only", http.StatusForbidden)
		return
	}
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusMethodNotAllowed)
		_, _ = w.Write([]byte(`{"error":{"code":"method_not_allowed","message":"This endpoint only accepts POST /v1/messages. 本机该路径只接受 POST /v1/messages.","type":"invalid_request_error"}}`))
		return
	}
	if !rt.ownerServing() {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusServiceUnavailable)
		_, _ = w.Write([]byte(`{"error":{"code":"bridge_stopping","message":"Messages listener is not serving.","type":"invalid_request_error"}}`))
		return
	}
	got := bearerToken(r.Header.Get("Authorization"))
	want := rt.ingressKey()
	if want == "" || got != want {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`{"error":{"code":"invalid_api_key","message":"Invalid local bearer token.","type":"invalid_request_error"}}`))
		return
	}

	body, err := io.ReadAll(io.LimitReader(r.Body, 8<<20))
	if err != nil {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusBadRequest)
		_, _ = w.Write([]byte(`{"error":{"code":"invalid_request","message":"Unable to read request body.","type":"invalid_request_error"}}`))
		return
	}
	var meta struct {
		Stream bool `json:"stream"`
	}
	_ = json.Unmarshal(body, &meta)

	upstream := strings.TrimRight(rt.upstreamBase(), "/") + "/v1/messages"
	req, err := http.NewRequestWithContext(r.Context(), http.MethodPost, upstream, bytes.NewReader(body))
	if err != nil {
		writeUpstreamUnavailable(w)
		return
	}
	req.Header.Set("Content-Type", "application/json")
	if meta.Stream {
		req.Header.Set("Accept", "text/event-stream")
	}
	rt.addInFlight(1)
	defer rt.addInFlight(-1)

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		writeUpstreamUnavailable(w)
		return
	}
	defer resp.Body.Close()

	for k, vs := range resp.Header {
		if strings.EqualFold(k, "Content-Length") {
			continue
		}
		for _, v := range vs {
			w.Header().Add(k, v)
		}
	}
	if w.Header().Get("Content-Type") == "" {
		if meta.Stream {
			w.Header().Set("Content-Type", "text/event-stream")
		} else {
			w.Header().Set("Content-Type", "application/json")
		}
	}
	w.WriteHeader(resp.StatusCode)
	if meta.Stream {
		copySSE(w, resp.Body)
		return
	}
	_, _ = io.Copy(w, resp.Body)
}

func bearerToken(header string) string {
	const prefix = "Bearer "
	if strings.HasPrefix(header, prefix) {
		return strings.TrimSpace(header[len(prefix):])
	}
	return ""
}

func writeUpstreamUnavailable(w http.ResponseWriter) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusBadGateway)
	_, _ = w.Write([]byte(`{"error":{"code":"upstream_unavailable","message":"Controlled loopback upstream is unavailable.","type":"api_error"}}`))
}

func copySSE(w http.ResponseWriter, r io.Reader) {
	flusher, _ := w.(http.Flusher)
	buf := make([]byte, 4096)
	for {
		n, err := r.Read(buf)
		if n > 0 {
			if _, writeErr := w.Write(buf[:n]); writeErr != nil {
				return
			}
			if flusher != nil {
				flusher.Flush()
			}
		}
		if err != nil {
			return
		}
	}
}
