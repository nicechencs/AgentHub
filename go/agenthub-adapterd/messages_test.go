package main

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
)

func activateProbe(t *testing.T, rt *Runtime, upstreamURL, key string) (epoch string, term int64, port int) {
	t.Helper()
	hs := handshakeOK(t, rt)
	acq := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "acq-probe",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "probe-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "lease_budget_ms": 60000},
	})
	if !acq.OK {
		t.Fatalf("acquire: %+v", acq.Error)
	}
	var acqPayload AcquireSuccess
	if err := json.Unmarshal(acq.Payload, &acqPayload); err != nil {
		t.Fatal(err)
	}
	fixture := ProbeFixture{
		IngressKey:      key,
		UpstreamBaseURL: upstreamURL,
		FixtureModel:    "claude-probe-fixture",
	}
	raw, _ := json.Marshal(fixture)
	if err := os.WriteFile(filepath.Join(rt.Home(), "config", "probe.json"), raw, 0o600); err != nil {
		t.Fatal(err)
	}
	act := controlJSON(t, rt, map[string]any{
		"type":           typeActivateProbeListen,
		"request_id":     "act-1",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "probe-owner",
		"owner_term":     acqPayload.OwnerTerm,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{},
	})
	if !act.OK {
		t.Fatalf("activate: %+v", act.Error)
	}
	st := controlJSON(t, rt, map[string]any{
		"type":           typeStatus,
		"request_id":     "st-act",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "probe-owner",
		"owner_term":     acqPayload.OwnerTerm,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{},
	})
	if !st.OK {
		t.Fatalf("status after activate: %+v", st.Error)
	}
	if bytes.Contains(st.Payload, []byte(key)) {
		t.Fatalf("status leaked synthetic key: %s", st.Payload)
	}
	var status StatusSuccess
	if err := json.Unmarshal(st.Payload, &status); err != nil {
		t.Fatal(err)
	}
	if !status.ListenReady || status.Lifecycle != lifecycleServing {
		t.Fatalf("expected serving, got %+v", status)
	}
	if status.Port == nil || *status.Port <= 0 {
		t.Fatal("status missing port")
	}
	return hs.InstanceEpoch, acqPayload.OwnerTerm, *status.Port
}

func TestMessagesJSONSyntheticKey(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(mockMessages))
	t.Cleanup(upstream.Close)
	rt := testRuntime(t)
	_, _, port := activateProbe(t, rt, upstream.URL, testIngressKey)

	body := []byte(`{"model":"claude-probe-fixture","max_tokens":16,"stream":false,"messages":[{"role":"user","content":"ping"}]}`)
	req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer "+testIngressKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status %d body %s", resp.StatusCode, got)
	}
	if !bytes.Contains(got, []byte(fixtureAssistantText)) {
		t.Fatalf("missing fixture text: %s", got)
	}

	bad, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	bad.Header.Set("Authorization", "Bearer wrong-key")
	badResp, err := http.DefaultClient.Do(bad)
	if err != nil {
		t.Fatal(err)
	}
	defer badResp.Body.Close()
	badBody, _ := io.ReadAll(badResp.Body)
	if badResp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("expected 401, got %d %s", badResp.StatusCode, badBody)
	}
	if !bytes.Contains(badBody, []byte("invalid_api_key")) {
		t.Fatalf("expected invalid_api_key: %s", badBody)
	}
}

func TestMessagesSSESyntheticKey(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(mockMessages))
	t.Cleanup(upstream.Close)
	rt := testRuntime(t)
	_, _, port := activateProbe(t, rt, upstream.URL, testIngressKey)

	body := []byte(`{"model":"claude-probe-fixture","max_tokens":16,"stream":true,"messages":[{"role":"user","content":"ping"}]}`)
	req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Authorization", "Bearer "+testIngressKey)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("status %d body %s", resp.StatusCode, got)
	}
	if !strings.Contains(resp.Header.Get("Content-Type"), "text/event-stream") {
		t.Fatalf("expected SSE content type, got %q", resp.Header.Get("Content-Type"))
	}
	if !bytes.Contains(got, []byte("event: content_block_delta")) || !bytes.Contains(got, []byte(fixtureAssistantText)) {
		t.Fatalf("missing SSE fixture: %s", got)
	}
}

func TestMessagesMethodNotAllowed(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(mockMessages))
	t.Cleanup(upstream.Close)
	rt := testRuntime(t)
	_, _, port := activateProbe(t, rt, upstream.URL, testIngressKey)
	resp, err := http.Get("http://127.0.0.1:" + strconv.Itoa(port) + "/v1/messages")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusMethodNotAllowed {
		t.Fatalf("expected 405, got %d %s", resp.StatusCode, got)
	}
	if resp.Header.Get("Allow") != "POST" {
		t.Fatalf("Allow=%q", resp.Header.Get("Allow"))
	}
	if !bytes.Contains(got, []byte("method_not_allowed")) {
		t.Fatalf("body %s", got)
	}
}
