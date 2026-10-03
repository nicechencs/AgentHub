package main

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const testIngressKey = "ahb_unit_test_ingress_key_secret_value"

func testRuntime(t *testing.T) *Runtime {
	t.Helper()
	dir, err := os.MkdirTemp("/tmp", "ah-ad-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(dir) })
	ctx, cancel := context.WithCancel(context.Background())
	t.Cleanup(cancel)
	rt, err := NewRuntime(dir, 0, filepath.Join(dir, "run", "adapterd.sock"), cancel)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = rt.Shutdown(ctx) })
	return rt
}

func controlJSON(t *testing.T, rt *Runtime, body any) Reply {
	t.Helper()
	raw, err := json.Marshal(body)
	if err != nil {
		t.Fatal(err)
	}
	return rt.HandleControl(raw)
}

func handshakeOK(t *testing.T, rt *Runtime) HandshakeSuccess {
	t.Helper()
	reply := controlJSON(t, rt, map[string]any{
		"type":         typeHandshake,
		"request_id":   "hs-1",
		"app_data_dir": rt.Home(),
		"payload": map[string]any{
			"protocol_version":      protocolVersion,
			"config_format_version": configFormatVersion,
			"package_version":       packageVersion,
			"app_data_dir":          rt.Home(),
		},
	})
	if !reply.OK {
		t.Fatalf("handshake failed: %+v", reply.Error)
	}
	var payload HandshakeSuccess
	if err := json.Unmarshal(reply.Payload, &payload); err != nil {
		t.Fatal(err)
	}
	if payload.InstanceEpoch == "" || payload.InstanceID == "" {
		t.Fatalf("handshake missing instance identity: %+v", payload)
	}
	if payload.Active != nil || payload.Prepared != nil {
		t.Fatalf("new epoch must start with null active/prepared: %+v", payload)
	}
	return payload
}

func TestHandshakeAndStatusEnvelope(t *testing.T) {
	rt := testRuntime(t)
	hs := handshakeOK(t, rt)

	reply := controlJSON(t, rt, map[string]any{
		"type":           typeStatus,
		"request_id":     "st-1",
		"instance_epoch": hs.InstanceEpoch,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{},
	})
	if !reply.OK {
		t.Fatalf("status failed: %+v", reply.Error)
	}
	var status StatusSuccess
	if err := json.Unmarshal(reply.Payload, &status); err != nil {
		t.Fatal(err)
	}
	if status.Lifecycle != lifecycleEmpty && status.Lifecycle != lifecycleNotServing {
		t.Fatalf("expected empty lifecycle before owner, got %s", status.Lifecycle)
	}
	if status.ListenReady {
		t.Fatal("listen must stay down until probe activate")
	}
	if status.OwnerLeaseValid {
		t.Fatal("owner lease must be false before acquire")
	}
	raw, _ := json.Marshal(status)
	for _, needle := range []string{"ingress_key", "local_token", testIngressKey, "api_key"} {
		if strings.Contains(strings.ToLower(string(raw)), needle) && needle != "api_key" {
			t.Fatalf("status leaked %q: %s", needle, raw)
		}
	}
	if strings.Contains(string(raw), testIngressKey) {
		t.Fatalf("status leaked synthetic key: %s", raw)
	}
}

func TestHandshakeRejectsVersionMismatchAndOwnerTerm(t *testing.T) {
	rt := testRuntime(t)
	reply := controlJSON(t, rt, map[string]any{
		"type":         typeHandshake,
		"request_id":   "hs-bad-ver",
		"app_data_dir": rt.Home(),
		"payload": map[string]any{
			"protocol_version": "not-this-slice",
			"app_data_dir":     rt.Home(),
		},
	})
	if reply.OK || reply.Error == nil || reply.Error.Code != errProtocolMismatch {
		t.Fatalf("expected protocol_mismatch, got %+v", reply)
	}

	term := int64(1)
	raw, _ := json.Marshal(Envelope{
		Type:       typeHandshake,
		RequestID:  "hs-bad-term",
		OwnerTerm:  &term,
		AppDataDir: rt.Home(),
		Payload: marshalPayload(HandshakePayload{
			ProtocolVersion: protocolVersion,
			AppDataDir:      rt.Home(),
		}),
	})
	reply = rt.HandleControl(raw)
	if reply.OK || reply.Error == nil || reply.Error.Code != errSecretOnControl {
		t.Fatalf("expected secret_on_control for Handshake owner_term, got %+v", reply)
	}
}

func TestAcquireThenStatusLifecycle(t *testing.T) {
	rt := testRuntime(t)
	hs := handshakeOK(t, rt)
	reply := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "acq-1",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "probe-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "lease_budget_ms": 60000},
	})
	if !reply.OK {
		t.Fatalf("acquire failed: %+v", reply.Error)
	}
	var acq AcquireSuccess
	if err := json.Unmarshal(reply.Payload, &acq); err != nil {
		t.Fatal(err)
	}
	if acq.OwnerTerm != 1 {
		t.Fatalf("first acquire must issue term 1, got %d", acq.OwnerTerm)
	}

	statusReply := controlJSON(t, rt, map[string]any{
		"type":           typeStatus,
		"request_id":     "st-2",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "probe-owner",
		"owner_term":     acq.OwnerTerm,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{},
	})
	if !statusReply.OK {
		t.Fatalf("status after acquire failed: %+v", statusReply.Error)
	}
	var status StatusSuccess
	if err := json.Unmarshal(statusReply.Payload, &status); err != nil {
		t.Fatal(err)
	}
	if status.OwnerTerm == nil || *status.OwnerTerm != 1 {
		t.Fatalf("status owner_term=%v", status.OwnerTerm)
	}
	if !status.OwnerLeaseValid {
		t.Fatal("owner lease should be valid after acquire")
	}
	if status.ListenReady || status.Lifecycle == lifecycleServing {
		t.Fatal("acquire must not start Messages listening")
	}
}

func TestControlRejectsIngressKeyField(t *testing.T) {
	rt := testRuntime(t)
	reply := rt.HandleControl([]byte(`{"type":"Status","request_id":"nope","ingress_key":"secret"}`))
	if reply.OK || reply.Error == nil || reply.Error.Code != errSecretOnControl {
		t.Fatalf("expected secret_on_control, got %+v", reply)
	}
}

func TestNewRuntimeRejectsRealHomeAndDefaultPort(t *testing.T) {
	userHome, err := os.UserHomeDir()
	if err != nil {
		t.Fatal(err)
	}
	_, err = NewRuntime(filepath.Join(userHome, ".agenthub"), 0, "", nil)
	if err == nil {
		t.Fatal("expected refuse real ~/.agenthub")
	}
	dir, err := os.MkdirTemp("/tmp", "ah-ad-")
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(dir) })
	_, err = NewRuntime(dir, productDefaultPort, filepath.Join(dir, "run", "adapterd.sock"), nil)
	if err == nil {
		t.Fatal("expected refuse product default port")
	}
}
