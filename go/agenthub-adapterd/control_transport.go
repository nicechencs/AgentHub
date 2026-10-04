package main

import (
	"crypto/sha256"
	"crypto/subtle"
	"encoding/base64"
	"fmt"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
)

const controlTokenEnvironment = "AGENTHUB_ADAPTERD_CONTROL_TOKEN"
const maxTCPControlConnections = 8

func consumeControlTokenEnvironment() (string, bool) {
	// Unsetenv prevents later child inheritance and ordinary environment reads.
	// A same-UID process may still inspect startup environment memory on some
	// systems; product wiring must replace env delivery with an inherited FD or
	// handle before enabling this transport as the desktop default.
	token, present := os.LookupEnv(controlTokenEnvironment)
	_ = os.Unsetenv(controlTokenEnvironment)
	return token, present
}

// ConfigureTCPControl selects the cross-platform authenticated control
// transport. Production CLI callers must request 127.0.0.1:0 so the child
// binds atomically; a fixed port remains available only to package-level tests.
// The token is deliberately accepted only in memory; main obtains it from the
// environment and removes that environment entry immediately.
func (rt *Runtime) ConfigureTCPControl(address, token string) error {
	normalized, err := validateTCPControl(address, token)
	if err != nil {
		return err
	}
	rt.controlNetwork = "tcp4"
	rt.controlAddress = normalized
	rt.controlToken = token
	return nil
}

func validateTCPControl(address, token string) (string, error) {
	host, portText, err := net.SplitHostPort(strings.TrimSpace(address))
	if err != nil || host != "127.0.0.1" {
		return "", fmt.Errorf("TCP control listener must use 127.0.0.1 with an explicit port")
	}
	port, err := strconv.Atoi(portText)
	if err != nil || port < 0 || port > 65535 {
		return "", fmt.Errorf("TCP control listener port must be between 0 and 65535")
	}
	decoded, err := base64.RawURLEncoding.DecodeString(token)
	if err != nil || len(decoded) != 32 || base64.RawURLEncoding.EncodeToString(decoded) != token {
		return "", fmt.Errorf("TCP control authentication token must be a canonical 256-bit base64url value")
	}
	return net.JoinHostPort(host, portText), nil
}

func (rt *Runtime) handshakeCapabilities() []string {
	capabilities := append([]string(nil), baseHandshakeCapabilities...)
	if rt.controlNetwork == "tcp4" {
		capabilities = append(capabilities, "control.http_loopback.auth.v1")
	}
	return capabilities
}

func (rt *Runtime) openControlListener() (net.Listener, func(), error) {
	if rt.controlNetwork == "tcp4" {
		ln, err := net.Listen("tcp4", rt.controlAddress)
		if err != nil {
			return nil, func() {}, fmt.Errorf("listen TCP control endpoint: %w", err)
		}
		address := ln.Addr().String()
		host, _, splitErr := net.SplitHostPort(address)
		if splitErr != nil || host != "127.0.0.1" {
			_ = ln.Close()
			return nil, func() {}, fmt.Errorf("TCP control listener did not bind exact IPv4 loopback")
		}
		rt.controlAddress = address
		return newLimitedControlListener(ln, maxTCPControlConnections), func() {}, nil
	}
	if err := os.RemoveAll(rt.controlSocket); err != nil {
		return nil, func() {}, fmt.Errorf("remove old control socket: %w", err)
	}
	ln, err := net.Listen("unix", rt.controlSocket)
	if err != nil {
		return nil, func() {}, fmt.Errorf("listen control socket: %w", err)
	}
	if err := os.Chmod(rt.controlSocket, 0o600); err != nil {
		_ = ln.Close()
		return nil, func() {}, fmt.Errorf("chmod control socket: %w", err)
	}
	return ln, func() { _ = os.Remove(rt.controlSocket) }, nil
}

type limitedControlListener struct {
	net.Listener
	active chan struct{}
}

func newLimitedControlListener(inner net.Listener, limit int) net.Listener {
	return &limitedControlListener{Listener: inner, active: make(chan struct{}, limit)}
}

func (ln *limitedControlListener) Accept() (net.Conn, error) {
	for {
		conn, err := ln.Listener.Accept()
		if err != nil {
			return nil, err
		}
		select {
		case ln.active <- struct{}{}:
			return &limitedControlConn{Conn: conn, release: func() { <-ln.active }}, nil
		default:
			_ = conn.Close()
		}
	}
}

type limitedControlConn struct {
	net.Conn
	once    sync.Once
	release func()
}

func (conn *limitedControlConn) Close() error {
	err := conn.Conn.Close()
	conn.once.Do(conn.release)
	return err
}

func (rt *Runtime) authenticatedControlHandler(next http.Handler) http.Handler {
	if rt.controlNetwork != "tcp4" {
		return next
	}
	expected := sha256.Sum256([]byte("Bearer " + rt.controlToken))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		values := r.Header.Values("Authorization")
		if len(values) != 1 {
			writeControlUnauthorized(w)
			return
		}
		presented := sha256.Sum256([]byte(values[0]))
		if subtle.ConstantTimeCompare(expected[:], presented[:]) != 1 {
			writeControlUnauthorized(w)
			return
		}
		next.ServeHTTP(w, r)
	})
}

func writeControlUnauthorized(w http.ResponseWriter) {
	w.Header().Set("Cache-Control", "no-store")
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.WriteHeader(http.StatusUnauthorized)
	_, _ = w.Write([]byte("unauthorized\n"))
}
