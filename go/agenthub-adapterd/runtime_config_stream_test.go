package main

import (
	"bytes"
	"context"
	"encoding/binary"
	"errors"
	"io"
	"strings"
	"testing"
)

const (
	streamIngressSecret  = "ahb_stream_ingress_secret_do_not_log"
	streamUpstreamSecret = "sk_stream_upstream_secret_do_not_log"
)

func TestRuntimeConfigStreamReadsMultipleFrames(t *testing.T) {
	first := streamRuntimeConfigJSON("edge-one", streamIngressSecret, streamUpstreamSecret)
	second := streamRuntimeConfigJSON("edge-two", streamIngressSecret+"-2", streamUpstreamSecret+"-2")
	var stream bytes.Buffer
	if err := writeRuntimeConfigFrame(&stream, first); err != nil {
		t.Fatalf("write first frame: %v", err)
	}
	if err := writeRuntimeConfigFrame(&stream, second); err != nil {
		t.Fatalf("write second frame: %v", err)
	}

	for index, want := range [][]byte{first, second} {
		config, raw, digest, err := readRuntimeConfigFrame(&stream)
		if err != nil {
			t.Fatalf("read frame %d: %v", index, err)
		}
		if config.Version != runtimeConfigVersion || len(config.Edges) != 1 {
			t.Fatalf("frame %d config=%+v", index, config)
		}
		if !bytes.Equal(raw, want) {
			t.Fatalf("frame %d raw payload changed", index)
		}
		if digest != runtimeConfigDigest(want) || len(digest) != sha256HexLength {
			t.Fatalf("frame %d digest=%q", index, digest)
		}
	}
	if config, raw, digest, err := readRuntimeConfigFrame(&stream); !errors.Is(err, io.EOF) || config != nil || raw != nil || digest != "" {
		t.Fatalf("final read=(%v, %q, %q, %v), want clean EOF", config, raw, digest, err)
	}
}

func TestRuntimeConfigStreamRejectsTruncatedFrames(t *testing.T) {
	payload := streamRuntimeConfigJSON("edge", streamIngressSecret, streamUpstreamSecret)
	var complete bytes.Buffer
	if err := writeRuntimeConfigFrame(&complete, payload); err != nil {
		t.Fatal(err)
	}
	framed := complete.Bytes()

	for _, size := range []int{1, 2, 3, len(framed) - 1} {
		config, raw, digest, err := readRuntimeConfigFrame(bytes.NewReader(framed[:size]))
		if err == nil || config != nil || raw != nil || digest != "" {
			t.Fatalf("size %d accepted truncated frame", size)
		}
		assertRuntimeConfigStreamErrorIsSafe(t, err)
	}
}

func TestRuntimeConfigStreamRejectsOversizeBeforeReadingPayload(t *testing.T) {
	var header [runtimeConfigFrameHeaderSize]byte
	binary.BigEndian.PutUint32(header[:], uint32(maxRuntimeConfigSize+1))
	config, raw, digest, err := readRuntimeConfigFrame(bytes.NewReader(header[:]))
	if err == nil || config != nil || raw != nil || digest != "" {
		t.Fatal("oversize frame was accepted")
	}
	assertRuntimeConfigStreamErrorIsSafe(t, err)

	if err := writeRuntimeConfigFrame(io.Discard, make([]byte, maxRuntimeConfigSize+1)); err == nil {
		t.Fatal("writer accepted oversize frame")
	}
}

func TestRuntimeConfigStreamInvalidPayloadErrorsDoNotEchoSecrets(t *testing.T) {
	invalidPayloads := [][]byte{
		[]byte(`{"secret":"` + streamIngressSecret),
		[]byte(`{"version":"route-config.v0-isolated","edges":[{"id":"` + streamIngressSecret + `","ingress_key":"` + streamIngressSecret + `","surface":"secret-unsupported","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"model","members":[{"id":"member","upstream_base_url":"http://127.0.0.1:18080","upstream_key":"` + streamUpstreamSecret + `","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages","models":["model"]}]}]}`),
	}
	for _, payload := range invalidPayloads {
		var stream bytes.Buffer
		if err := writeRuntimeConfigFrame(&stream, payload); err != nil {
			t.Fatalf("write invalid frame: %v", err)
		}
		config, raw, digest, err := readRuntimeConfigFrame(&stream)
		if err == nil || config != nil || raw != nil || digest != "" {
			t.Fatal("invalid frame was accepted")
		}
		assertRuntimeConfigStreamErrorIsSafe(t, err)
	}
}

func TestRuntimeConfigStreamWriteErrorDoesNotEchoSecrets(t *testing.T) {
	err := writeRuntimeConfigFrame(secretErrorWriter{}, []byte(streamUpstreamSecret))
	if err == nil {
		t.Fatal("writer error was not returned")
	}
	assertRuntimeConfigStreamErrorIsSafe(t, err)
}

func TestRuntimeConfigDigestIsLowercaseSHA256(t *testing.T) {
	const want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
	if got := runtimeConfigDigest([]byte("abc")); got != want {
		t.Fatalf("digest=%q, want %q", got, want)
	}
}

func TestRuntimeConfigConsumerKeepsLastGoodConfigAfterInvalidFrame(t *testing.T) {
	initialRaw := streamRuntimeConfigJSON("edge-one", streamIngressSecret, streamUpstreamSecret)
	initial, err := LoadRuntimeConfig(bytes.NewReader(initialRaw))
	if err != nil {
		t.Fatal(err)
	}
	canceled := make(chan struct{})
	rt, err := NewRuntime(t.TempDir(), 0, "", func() { close(canceled) })
	if err != nil {
		t.Fatal(err)
	}
	if err := rt.SetRuntimeConfigWithDigest(initial, runtimeConfigDigest(initialRaw)); err != nil {
		t.Fatal(err)
	}

	updatedRaw := streamRuntimeConfigJSON("edge-two", streamIngressSecret+"-2", streamUpstreamSecret+"-2")
	var stream bytes.Buffer
	if err := writeRuntimeConfigFrame(&stream, []byte(`{"version":"bad"}`)); err != nil {
		t.Fatal(err)
	}
	if err := writeRuntimeConfigFrame(&stream, updatedRaw); err != nil {
		t.Fatal(err)
	}
	consumeRuntimeConfigFrames(context.Background(), rt, &stream)

	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.configHash != runtimeConfigDigest(updatedRaw) || rt.configRevision != 2 {
		t.Fatalf("active config=(%q,%d), want accepted second frame", rt.configHash, rt.configRevision)
	}
	if len(rt.edges) != 1 || rt.edges[0].ID != "edge-two" {
		t.Fatalf("active edges=%+v", rt.edges)
	}
	select {
	case <-canceled:
	default:
		t.Fatal("stream EOF did not stop the runtime")
	}
}

func TestRuntimeConfigConsumerFailsClosedAfterRejectionBudget(t *testing.T) {
	initialRaw := streamRuntimeConfigJSON("edge-one", streamIngressSecret, streamUpstreamSecret)
	initial, err := LoadRuntimeConfig(bytes.NewReader(initialRaw))
	if err != nil {
		t.Fatal(err)
	}
	canceled := make(chan struct{})
	rt, err := NewRuntime(t.TempDir(), 0, "", func() { close(canceled) })
	if err != nil {
		t.Fatal(err)
	}
	if err := rt.SetRuntimeConfigWithDigest(initial, runtimeConfigDigest(initialRaw)); err != nil {
		t.Fatal(err)
	}
	rt.mu.Lock()
	rt.listenReady = true
	rt.lifecycle = lifecycleServing
	rt.mu.Unlock()

	var stream bytes.Buffer
	for range maxConsecutiveRuntimeConfigRejections {
		if err := writeRuntimeConfigFrame(&stream, []byte(`{"version":"bad"}`)); err != nil {
			t.Fatal(err)
		}
	}
	consumeRuntimeConfigFrames(context.Background(), rt, &stream)

	select {
	case <-canceled:
	default:
		t.Fatal("rejection budget did not stop the runtime")
	}
	snapshot, err := rt.statusSnapshot()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.ListenReady || snapshot.Lifecycle != lifecycleNotServing || snapshot.LastError == nil || snapshot.LastError.Code != errConfigStream {
		t.Fatalf("runtime did not fail closed: %+v", snapshot)
	}
	if snapshot.ActiveRevision == nil || *snapshot.ActiveRevision != "1" || snapshot.ActiveHash == nil || *snapshot.ActiveHash != runtimeConfigDigest(initialRaw) {
		t.Fatalf("last good config identity changed: %+v", snapshot)
	}
}

const sha256HexLength = 64

type secretErrorWriter struct{}

func (secretErrorWriter) Write([]byte) (int, error) {
	return 0, errors.New(streamIngressSecret + streamUpstreamSecret)
}

func streamRuntimeConfigJSON(edgeID, ingressKey, upstreamKey string) []byte {
	return []byte(`{"version":"route-config.v0-isolated","edges":[{"id":"` + edgeID + `","ingress_key":"` + ingressKey + `","surface":"messages","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"claude-stream-model","members":[{"id":"member","upstream_base_url":"http://127.0.0.1:18080","upstream_key":"` + upstreamKey + `","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages","priority":0,"position":0,"models":["claude-stream-model"]}]}]}`)
}

func assertRuntimeConfigStreamErrorIsSafe(t *testing.T, err error) {
	t.Helper()
	message := err.Error()
	for _, secret := range []string{streamIngressSecret, streamUpstreamSecret} {
		if strings.Contains(message, secret) {
			t.Fatalf("error leaked secret: %q", message)
		}
	}
}
