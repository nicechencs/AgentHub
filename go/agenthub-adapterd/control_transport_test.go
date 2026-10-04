package main

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

type observedBody struct {
	read bool
	data string
}

func (body *observedBody) Read(p []byte) (int, error) {
	body.read = true
	if body.data == "" {
		return 0, io.EOF
	}
	n := copy(p, body.data)
	body.data = body.data[n:]
	return n, nil
}

func (body *observedBody) Close() error { return nil }

func testControlToken() string {
	return base64.RawURLEncoding.EncodeToString([]byte("0123456789abcdef0123456789abcdef"))
}

func TestConsumeControlTokenEnvironmentUnsetsImmediately(t *testing.T) {
	token := testControlToken()
	t.Setenv(controlTokenEnvironment, token)
	got, present := consumeControlTokenEnvironment()
	if !present || got != token {
		t.Fatal("control token was not consumed")
	}
	if _, stillPresent := os.LookupEnv(controlTokenEnvironment); stillPresent {
		t.Fatal("control token remained in process environment")
	}
}

func TestConfigureTCPControlRequiresIPv4LoopbackAnd256BitToken(t *testing.T) {
	rt := testRuntime(t)
	token := testControlToken()
	for _, address := range []string{
		"localhost:1234",
		"0.0.0.0:1234",
		"[::1]:1234",
		"127.0.0.2:1234",
		"127.0.0.1:70000",
		"127.0.0.1",
	} {
		if err := rt.ConfigureTCPControl(address, token); err == nil {
			t.Fatalf("accepted unsafe TCP control address %q", address)
		}
	}
	for _, token := range []string{"", "short", base64.StdEncoding.EncodeToString(make([]byte, 32))} {
		if err := rt.ConfigureTCPControl("127.0.0.1:1234", token); err == nil {
			t.Fatal("accepted a non-canonical 256-bit control token")
		}
	}
	if err := rt.ConfigureTCPControl("127.0.0.1:1234", token); err != nil {
		t.Fatalf("configure valid TCP control endpoint: %v", err)
	}
	network, address := rt.ControlEndpoint()
	if network != "tcp4" || address != "127.0.0.1:1234" {
		t.Fatalf("unexpected endpoint %s %s", network, address)
	}
	if err := rt.ConfigureTCPControl("127.0.0.1:0", token); err != nil {
		t.Fatalf("configure child-selected TCP control port: %v", err)
	}
}

func TestNewTCPRuntimeDoesNotApplyUnixSocketPathLimit(t *testing.T) {
	longComponent := strings.Repeat("a", maxUnixSocketBytes)
	home := filepath.Join(t.TempDir(), longComponent)
	rt, err := NewTCPRuntime(home, 0, "127.0.0.1:1234", testControlToken(), func() {})
	if err != nil {
		t.Fatalf("TCP runtime incorrectly applied Unix socket validation: %v", err)
	}
	t.Cleanup(func() { _ = rt.Shutdown(context.Background()) })
}

func TestTCPListenerBindsEphemeralPortAndLimitsConnections(t *testing.T) {
	rt, err := NewTCPRuntime(t.TempDir(), 0, "127.0.0.1:0", testControlToken(), func() {})
	if err != nil {
		t.Fatal(err)
	}
	ln, cleanup, err := rt.openControlListener()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { cleanup(); _ = ln.Close() })
	_, port, err := net.SplitHostPort(rt.controlAddress)
	if err != nil || port == "0" {
		t.Fatalf("listener did not publish its selected port: %q", rt.controlAddress)
	}

	raw, err := net.Listen("tcp4", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	limited := newLimitedControlListener(raw, 1)
	t.Cleanup(func() { _ = limited.Close() })
	clientOne, err := net.Dial("tcp4", raw.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	serverOne, err := limited.Accept()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = clientOne.Close(); _ = serverOne.Close() })
	clientTwo, err := net.Dial("tcp4", raw.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = clientTwo.Close() })
	accepted := make(chan net.Conn, 1)
	acceptErr := make(chan error, 1)
	go func() {
		conn, err := limited.Accept()
		if err != nil {
			acceptErr <- err
			return
		}
		accepted <- conn
	}()
	if err := clientTwo.SetReadDeadline(time.Now().Add(time.Second)); err != nil {
		t.Fatal(err)
	}
	if _, err := clientTwo.Read(make([]byte, 1)); err == nil {
		t.Fatal("connection beyond the control limit remained open")
	}
	_ = serverOne.Close()
	clientThree, err := net.Dial("tcp4", raw.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = clientThree.Close() })
	select {
	case conn := <-accepted:
		_ = conn.Close()
	case err := <-acceptErr:
		t.Fatalf("accept after releasing a slot: %v", err)
	case <-time.After(time.Second):
		t.Fatal("connection slot was not released")
	}
}

func TestHandshakeAdvertisesTCPAuthenticationOnlyOnTCP(t *testing.T) {
	uds := testRuntime(t)
	udsHandshake := handshakeOK(t, uds)
	for _, capability := range udsHandshake.Capabilities {
		if capability == "control.http_loopback.auth.v1" {
			t.Fatal("Unix socket handshake advertised TCP authentication")
		}
	}

	tcp := testRuntime(t)
	if err := tcp.ConfigureTCPControl("127.0.0.1:0", testControlToken()); err != nil {
		t.Fatal(err)
	}
	tcpHandshake := handshakeOK(t, tcp)
	raw, err := json.Marshal(tcpHandshake.Capabilities)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(raw), "control.http_loopback.auth.v1") {
		t.Fatalf("TCP handshake omitted authentication capability: %s", raw)
	}
}

func TestTCPControlAuthenticatesBeforeReadingOrMutating(t *testing.T) {
	rt := testRuntime(t)
	token := testControlToken()
	if err := rt.ConfigureTCPControl("127.0.0.1:1234", token); err != nil {
		t.Fatal(err)
	}
	handshake := `{"type":"Handshake","request_id":"tcp-hs","app_data_dir":"` + rt.Home() +
		`","payload":{"protocol_version":"` + protocolVersion + `","config_format_version":"` +
		configFormatVersion + `","package_version":"` + packageVersion + `","app_data_dir":"` + rt.Home() + `"}}`

	cases := []struct {
		name    string
		headers []string
	}{
		{name: "missing"},
		{name: "wrong", headers: []string{"Bearer " + strings.Repeat("x", len(token))}},
		{name: "duplicate", headers: []string{"Bearer " + token, "Bearer " + token}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			body := &observedBody{data: handshake}
			req := httptest.NewRequest(http.MethodPost, "http://127.0.0.1/control", body)
			req.Header["Authorization"] = tc.headers
			recorder := httptest.NewRecorder()
			rt.controlHTTPHandler().ServeHTTP(recorder, req)
			if recorder.Code != http.StatusUnauthorized {
				t.Fatalf("status=%d body=%q", recorder.Code, recorder.Body.String())
			}
			if body.read {
				t.Fatal("unauthenticated request body was read")
			}
			rt.mu.Lock()
			handshaked := rt.handshaked
			entries := len(rt.idempotency)
			rt.mu.Unlock()
			if handshaked || entries != 0 {
				t.Fatalf("unauthenticated request mutated runtime: handshaked=%v entries=%d", handshaked, entries)
			}
		})
	}

	req := httptest.NewRequest(http.MethodPost, "http://127.0.0.1/control", strings.NewReader(handshake))
	req.Header.Set("Authorization", "Bearer "+token)
	recorder := httptest.NewRecorder()
	rt.controlHTTPHandler().ServeHTTP(recorder, req)
	if recorder.Code != http.StatusOK || !strings.Contains(recorder.Body.String(), `"ok":true`) {
		t.Fatalf("authorized handshake status=%d body=%q", recorder.Code, recorder.Body.String())
	}

	health := httptest.NewRequest(http.MethodGet, "http://127.0.0.1/healthz", nil)
	healthRecorder := httptest.NewRecorder()
	rt.controlHTTPHandler().ServeHTTP(healthRecorder, health)
	if healthRecorder.Code != http.StatusUnauthorized {
		t.Fatalf("unauthenticated health status=%d", healthRecorder.Code)
	}
	health.Header.Set("Authorization", "Bearer "+token)
	healthRecorder = httptest.NewRecorder()
	rt.controlHTTPHandler().ServeHTTP(healthRecorder, health)
	if healthRecorder.Code != http.StatusOK {
		t.Fatalf("authenticated health status=%d", healthRecorder.Code)
	}
}
