package main

import (
	"bytes"
	"encoding/binary"
	"errors"
	"os"
	"strings"
	"testing"
)

func framedControlToken(declared uint32, payload string) []byte {
	frame := make([]byte, 4, 4+len(payload))
	binary.BigEndian.PutUint32(frame, declared)
	return append(frame, payload...)
}

func TestRejectLegacyControlTokenEnvironmentUnsetsImmediately(t *testing.T) {
	t.Setenv(controlTokenEnvironment, testControlToken())
	if !rejectLegacyControlTokenEnvironment() {
		t.Fatal("legacy control token environment was not rejected")
	}
	if _, present := os.LookupEnv(controlTokenEnvironment); present {
		t.Fatal("legacy control token environment remained set")
	}
	if rejectLegacyControlTokenEnvironment() {
		t.Fatal("absent legacy control token environment was reported present")
	}
}

func TestReadControlTokenPreludeLeavesRuntimeConfigBytes(t *testing.T) {
	token := testControlToken()
	reader := bytes.NewBuffer(append(framedControlToken(controlTokenEncodedLength, token), []byte("runtime-config")...))
	got, err := readControlTokenPrelude(reader)
	if err != nil {
		t.Fatal(err)
	}
	if got != token {
		t.Fatal("control token prelude changed the token")
	}
	if got := reader.String(); got != "runtime-config" {
		t.Fatalf("control token prelude consumed runtime config bytes: %q", got)
	}
}

func TestReadControlTokenPreludeRejectsMalformedFramesWithoutLeakingToken(t *testing.T) {
	token := testControlToken()
	cases := []struct {
		name  string
		frame []byte
	}{
		{name: "empty"},
		{name: "short header", frame: []byte{0, 0, 0}},
		{name: "zero length", frame: framedControlToken(0, "")},
		{name: "length 42", frame: framedControlToken(42, token[:42])},
		{name: "length 44", frame: framedControlToken(44, token+"x")},
		{name: "short payload", frame: framedControlToken(controlTokenEncodedLength, token[:42])},
		{name: "non canonical", frame: framedControlToken(controlTokenEncodedLength, "!"+token[1:])},
		{name: "newline", frame: framedControlToken(controlTokenEncodedLength, token[:42]+"\n")},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			_, err := readControlTokenPrelude(bytes.NewReader(tc.frame))
			if !errors.Is(err, errControlTokenSource) {
				t.Fatalf("error=%v, want generic control source rejection", err)
			}
			if strings.Contains(err.Error(), token) {
				t.Fatal("control token leaked through error")
			}
		})
	}
}

func TestConsumeControlTokenStdinRequiresExactTCPPairing(t *testing.T) {
	token := testControlToken()
	frame := framedControlToken(controlTokenEncodedLength, token)

	got, err := consumeControlTokenStdin(false, false, nil)
	if err != nil || got != "" {
		t.Fatalf("Unix control unexpectedly consumed a token: token=%q error=%v", got, err)
	}
	for _, tc := range []struct {
		name       string
		tcp, stdin bool
	}{
		{name: "TCP missing token prelude flag", tcp: true},
		{name: "Unix with token prelude flag", stdin: true},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := consumeControlTokenStdin(tc.tcp, tc.stdin, bytes.NewReader(frame)); !errors.Is(err, errControlTokenSource) {
				t.Fatalf("error=%v, want generic control source rejection", err)
			}
		})
	}
	got, err = consumeControlTokenStdin(true, true, bytes.NewReader(frame))
	if err != nil || got != token {
		t.Fatalf("valid TCP token prelude rejected: token match=%v error=%v", got == token, err)
	}
}
