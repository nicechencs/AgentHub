package main

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

const (
	oauthTestIngress = "ahb_oauth_refresh_ingress_synthetic"
	oauthTestOldKey  = "oauth_old_access_synthetic"
	oauthTestNewKey  = "oauth_new_access_synthetic"
	oauthTestModel   = "gpt-oauth-refresh"
)

func oauthRuntimeConfig(upstream, key, refreshKind string) *RuntimeConfig {
	return &RuntimeConfig{
		Version: runtimeConfigVersion,
		Edges: []RuntimeEdgeConfig{{
			ID:             "oauth-edge",
			IngressKey:     oauthTestIngress,
			Surface:        surfaceResponses,
			Dialect:        "codex",
			SchedulePolicy: policyPriorityFailover,
			FixtureModel:   oauthTestModel,
			Members: []RuntimeMemberConfig{{
				ID:                "account:oauth-account",
				SourceKind:        "account",
				SourceID:          "oauth-account",
				RefreshKind:       refreshKind,
				UpstreamBaseURL:   upstream,
				UpstreamKey:       key,
				UpstreamAuth:      authBearer,
				UpstreamTransport: transportCodexResponses,
				Models:            []string{oauthTestModel},
			}},
		}},
	}
}

func ownedStartedOAuthRuntime(t *testing.T, config *RuntimeConfig, digest string) (*Runtime, string, int64, int) {
	t.Helper()
	rt := testRuntime(t)
	if err := rt.SetRuntimeConfigWithDigest(config, digest); err != nil {
		t.Fatal(err)
	}
	hs := handshakeOK(t, rt)
	acquire := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "oauth-acquire",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "oauth-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "lease_budget_ms": 60000},
	})
	if !acquire.OK {
		t.Fatalf("acquire: %+v", acquire.Error)
	}
	var acquired AcquireSuccess
	if err := json.Unmarshal(acquire.Payload, &acquired); err != nil {
		t.Fatal(err)
	}
	start := controlJSON(t, rt, map[string]any{
		"type":           typeStart,
		"request_id":     "oauth-start",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "oauth-owner",
		"owner_term":     acquired.OwnerTerm,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"listen_port": 0},
	})
	if !start.OK {
		t.Fatalf("start: %+v", start.Error)
	}
	var started struct {
		Port int `json:"port"`
	}
	if err := json.Unmarshal(start.Payload, &started); err != nil {
		t.Fatal(err)
	}
	return rt, hs.InstanceEpoch, acquired.OwnerTerm, started.Port
}

func nextOAuthEvent(t *testing.T, rt *Runtime, epoch string, term int64, requestID string, waitMS int64) Reply {
	t.Helper()
	return controlJSON(t, rt, map[string]any{
		"type":           typeNextOAuthRefresh,
		"request_id":     requestID,
		"instance_epoch": epoch,
		"owner_id":       "oauth-owner",
		"owner_term":     term,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"wait_ms": waitMS},
	})
}

func decodeOAuthEvent(t *testing.T, reply Reply) *OAuthRefreshEvent {
	t.Helper()
	if !reply.OK {
		t.Fatalf("next refresh: %+v", reply.Error)
	}
	var payload NextOAuthRefreshSuccess
	if err := json.Unmarshal(reply.Payload, &payload); err != nil {
		t.Fatal(err)
	}
	return payload.Event
}

func completeOAuthEvent(t *testing.T, rt *Runtime, epoch string, term int64, requestID string, event *OAuthRefreshEvent, appliedHash, outcome string) Reply {
	t.Helper()
	return controlJSON(t, rt, map[string]any{
		"type":           typeCompleteOAuthRefresh,
		"request_id":     requestID,
		"instance_epoch": epoch,
		"owner_id":       "oauth-owner",
		"owner_term":     term,
		"app_data_dir":   rt.Home(),
		"payload": map[string]any{
			"refresh_id":          event.RefreshID,
			"active_hash":         event.ActiveHash,
			"applied_active_hash": appliedHash,
			"edge_id":             event.EdgeID,
			"member_id":           event.MemberID,
			"source_kind":         event.SourceKind,
			"source_id":           event.SourceID,
			"refresh_kind":        event.RefreshKind,
			"outcome":             outcome,
		},
	})
}

func firstOAuthMember(t *testing.T, rt *Runtime) *PoolMember {
	t.Helper()
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if len(rt.edges) != 1 {
		t.Fatalf("edges=%d", len(rt.edges))
	}
	member := rt.edges[0].Pool.MemberByIdentity("account:oauth-account", "account", "oauth-account", refreshCodexOAuth)
	if member == nil {
		t.Fatal("missing OAuth member")
	}
	return member
}

func oauthMemberByIdentity(t *testing.T, rt *Runtime, memberID, sourceID string) *PoolMember {
	t.Helper()
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if len(rt.edges) != 1 {
		t.Fatalf("edges=%d", len(rt.edges))
	}
	member := rt.edges[0].Pool.MemberByIdentity(memberID, "account", sourceID, refreshCodexOAuth)
	if member == nil {
		t.Fatalf("missing OAuth member %s", memberID)
	}
	return member
}

func TestRuntimeConfigOAuthRefreshIdentityValidation(t *testing.T) {
	valid := oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth)
	if err := validateRuntimeConfig(valid); err != nil {
		t.Fatal(err)
	}

	mutate := func(fn func(*RuntimeMemberConfig)) *RuntimeConfig {
		candidate := oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth)
		fn(&candidate.Edges[0].Members[0])
		return candidate
	}
	for name, candidate := range map[string]*RuntimeConfig{
		"provider":        mutate(func(member *RuntimeMemberConfig) { member.SourceKind = "provider" }),
		"missing source":  mutate(func(member *RuntimeMemberConfig) { member.SourceID = "" }),
		"wrong transport": mutate(func(member *RuntimeMemberConfig) { member.UpstreamTransport = transportOpenAIChatCompletions }),
		"wrong refresh":   mutate(func(member *RuntimeMemberConfig) { member.RefreshKind = "oauth" }),
		"duplicate id": func() *RuntimeConfig {
			candidate := oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth)
			candidate.Edges[0].Members = append(candidate.Edges[0].Members, candidate.Edges[0].Members[0])
			return candidate
		}(),
	} {
		t.Run(name, func(t *testing.T) {
			if err := validateRuntimeConfig(candidate); err == nil {
				t.Fatal("accepted invalid OAuth refresh identity")
			} else if strings.Contains(err.Error(), oauthTestOldKey) {
				t.Fatalf("validation leaked login information: %v", err)
			}
		})
	}

	apiKey := threeEdgeConfig("http://127.0.0.1:18080")
	apiKey.Edges[0].Members[0].SourceKind = "account"
	apiKey.Edges[0].Members[0].SourceID = "api-key-account"
	apiKey.Edges[0].Members[0].RefreshKind = refreshNone
	if err := validateRuntimeConfig(apiKey); err != nil {
		t.Fatalf("ordinary API Key member was rejected: %v", err)
	}
}

func TestOwnerReacquireUsesMonotonicTermAndCancelsPendingRefresh(t *testing.T) {
	rt, epoch, term, _ := ownedStartedOAuthRuntime(t, oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth), "hash-owner-old")
	member := firstOAuthMember(t, rt)
	result := make(chan *oauthRefreshRetry, 1)
	go func() { result <- rt.requestOAuthRefresh(context.Background(), "oauth-edge", member) }()
	if event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "owner-next", 1000)); event == nil {
		t.Fatal("missing refresh event")
	}

	rt.mu.Lock()
	rt.ownerLeaseUntil = time.Now().Add(-time.Second)
	rt.mu.Unlock()
	missingFence := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "owner-reacquire-without-fence",
		"instance_epoch": epoch,
		"owner_id":       "replacement-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "lease_budget_ms": 60000},
	})
	if missingFence.OK || missingFence.Error == nil || missingFence.Error.Code != errStaleTerm {
		t.Fatalf("unfenced reacquire=%+v", missingFence)
	}
	reacquire := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "owner-reacquire",
		"instance_epoch": epoch,
		"owner_id":       "replacement-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "previous_term": term, "lease_budget_ms": 60000},
	})
	if !reacquire.OK {
		t.Fatalf("reacquire: %+v", reacquire.Error)
	}
	var acquired AcquireSuccess
	if err := json.Unmarshal(reacquire.Payload, &acquired); err != nil {
		t.Fatal(err)
	}
	if acquired.OwnerTerm != term+1 {
		t.Fatalf("owner term=%d, want %d", acquired.OwnerTerm, term+1)
	}
	rt.mu.Lock()
	rt.ownerLeaseUntil = time.Now().Add(-time.Second)
	rt.mu.Unlock()
	delayedOldAcquire := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "owner-delayed-old-acquire",
		"instance_epoch": epoch,
		"owner_id":       "oauth-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "previous_term": term, "lease_budget_ms": 60000},
	})
	if delayedOldAcquire.OK || delayedOldAcquire.Error == nil || delayedOldAcquire.Error.Code != errStaleTerm {
		t.Fatalf("delayed old acquire crossed owner fence: %+v", delayedOldAcquire)
	}
	thirdAcquire := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "owner-third-acquire",
		"instance_epoch": epoch,
		"owner_id":       "third-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "previous_term": acquired.OwnerTerm, "lease_budget_ms": 60000},
	})
	if !thirdAcquire.OK {
		t.Fatalf("fenced third acquire: %+v", thirdAcquire.Error)
	}
	select {
	case retry := <-result:
		if retry != nil {
			t.Fatal("owner replacement allowed stale refresh retry")
		}
	case <-time.After(time.Second):
		t.Fatal("owner replacement did not cancel pending refresh")
	}
}

func TestOAuthRefreshControlBindsOwnerIdentityHashAndNonce(t *testing.T) {
	rt, epoch, term, _ := ownedStartedOAuthRuntime(t, oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth), "hash-control-old")
	member := firstOAuthMember(t, rt)
	rt.mu.Lock()
	oldEdge := rt.edges[0]
	rt.mu.Unlock()
	result := make(chan *oauthRefreshRetry, 1)
	go func() { result <- rt.requestOAuthRefresh(context.Background(), "oauth-edge", member) }()

	wrongOwner := controlJSON(t, rt, map[string]any{
		"type": typeNextOAuthRefresh, "request_id": "wrong-owner", "instance_epoch": epoch,
		"owner_id": "not-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{},
	})
	if wrongOwner.OK || wrongOwner.Error == nil || wrongOwner.Error.Code != errNotOwner {
		t.Fatalf("wrong owner reply=%+v", wrongOwner)
	}

	event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "control-next", 1000))
	if event == nil || event.ActiveHash != "hash-control-old" || event.SourceID != "oauth-account" || event.RefreshKind != refreshCodexOAuth {
		t.Fatalf("event=%+v", event)
	}
	rawEvent, _ := json.Marshal(event)
	for _, secret := range []string{oauthTestOldKey, oauthTestNewKey, oauthTestIngress} {
		if bytes.Contains(rawEvent, []byte(secret)) {
			t.Fatalf("refresh event leaked secret: %s", rawEvent)
		}
	}
	replayedAsOtherOwner := controlJSON(t, rt, map[string]any{
		"type": typeNextOAuthRefresh, "request_id": "control-next", "instance_epoch": epoch,
		"owner_id": "not-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{"wait_ms": 1000},
	})
	if replayedAsOtherOwner.OK || replayedAsOtherOwner.Error == nil || replayedAsOtherOwner.Error.Code != errInvalidRequest {
		t.Fatalf("request id replay crossed owner scope: %+v", replayedAsOtherOwner)
	}

	stale := completeOAuthEvent(t, rt, "stale-epoch", term, "stale-complete", event, "hash-control-new", oauthRefreshApplied)
	if stale.OK || stale.Error == nil || stale.Error.Code != errStaleEpoch {
		t.Fatalf("stale completion=%+v", stale)
	}
	mismatch := *event
	mismatch.MemberID = "account:other"
	bad := completeOAuthEvent(t, rt, epoch, term, "mismatch-complete", &mismatch, "hash-control-new", oauthRefreshApplied)
	if bad.OK || bad.Error == nil || bad.Error.Code != errOAuthRefreshMismatch {
		t.Fatalf("mismatch completion=%+v", bad)
	}

	if err := rt.SwapRuntimeConfig(oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestNewKey, refreshCodexOAuth), "hash-control-new"); err != nil {
		t.Fatal(err)
	}
	complete := completeOAuthEvent(t, rt, epoch, term, "control-complete", event, "hash-control-new", oauthRefreshApplied)
	if !complete.OK {
		t.Fatalf("complete: %+v", complete.Error)
	}
	var completed CompleteOAuthRefreshSuccess
	if err := json.Unmarshal(complete.Payload, &completed); err != nil {
		t.Fatal(err)
	}
	if !completed.Completed || !completed.RetryEligible {
		t.Fatalf("completed=%+v", completed)
	}
	select {
	case retry := <-result:
		if retry == nil || retry.member.UpstreamKey != oauthTestNewKey {
			t.Fatal("completion did not resolve the changed current member")
		}
		if retry.edge == nil || retry.edge == oldEdge || retry.edge.Pool != retry.pool {
			t.Fatal("completion did not return the refreshed edge generation")
		}
	case <-time.After(time.Second):
		t.Fatal("refresh waiter did not finish")
	}

	replay := completeOAuthEvent(t, rt, epoch, term, "control-replay", event, "hash-control-new", oauthRefreshApplied)
	if replay.OK || replay.Error == nil || replay.Error.Code != errOAuthRefreshMismatch {
		t.Fatalf("nonce replay=%+v", replay)
	}
}

func TestNextOAuthRefreshLongPollHonorsContextCancellation(t *testing.T) {
	rt, epoch, term, _ := ownedStartedOAuthRuntime(t, oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth), "hash-cancel")
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	raw, err := json.Marshal(map[string]any{
		"type": typeNextOAuthRefresh, "request_id": "cancel-next", "instance_epoch": epoch,
		"owner_id": "oauth-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{"wait_ms": 5000},
	})
	if err != nil {
		t.Fatal(err)
	}
	started := time.Now()
	reply := rt.HandleControlContext(ctx, raw)
	if reply.OK || reply.Error == nil || reply.Error.Code != errOAuthRefreshCanceled {
		t.Fatalf("canceled long poll=%+v", reply)
	}
	if time.Since(started) > time.Second {
		t.Fatal("canceled long poll did not return promptly")
	}
}

func TestConfigSwapCancelsUndeliveredOAuthRefresh(t *testing.T) {
	rt, _, _, _ := ownedStartedOAuthRuntime(t, oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth), "hash-undelivered-old")
	member := firstOAuthMember(t, rt)
	result := make(chan *oauthRefreshRetry, 1)
	go func() { result <- rt.requestOAuthRefresh(context.Background(), "oauth-edge", member) }()

	deadline := time.Now().Add(time.Second)
	for {
		rt.mu.Lock()
		pending := len(rt.oauthPendingByID)
		rt.mu.Unlock()
		if pending == 1 {
			break
		}
		if time.Now().After(deadline) {
			t.Fatal("refresh event was not queued")
		}
		time.Sleep(time.Millisecond)
	}
	if err := rt.SwapRuntimeConfig(oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestNewKey, refreshCodexOAuth), "hash-undelivered-new"); err != nil {
		t.Fatal(err)
	}
	select {
	case retry := <-result:
		if retry != nil {
			t.Fatal("undelivered stale event allowed retry")
		}
	case <-time.After(time.Second):
		t.Fatal("config swap did not cancel undelivered event")
	}
}

func TestOAuthRefreshClaimLeaseBoundsRedeliveryWithoutBlockingNewEvent(t *testing.T) {
	config := oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth)
	second := config.Edges[0].Members[0]
	second.ID = "account:oauth-account-2"
	second.SourceID = "oauth-account-2"
	second.UpstreamKey = "oauth_second_access_synthetic"
	second.Position = 1
	config.Edges[0].Members = append(config.Edges[0].Members, second)
	rt, epoch, term, _ := ownedStartedOAuthRuntime(t, config, "hash-claim-lease")
	first := oauthMemberByIdentity(t, rt, "account:oauth-account", "oauth-account")
	secondMember := oauthMemberByIdentity(t, rt, "account:oauth-account-2", "oauth-account-2")

	firstCtx, cancelFirst := context.WithCancel(context.Background())
	secondCtx, cancelSecond := context.WithCancel(context.Background())
	defer cancelFirst()
	defer cancelSecond()
	firstResult := make(chan *oauthRefreshRetry, 1)
	secondResult := make(chan *oauthRefreshRetry, 1)
	go func() { firstResult <- rt.requestOAuthRefresh(firstCtx, "oauth-edge", first) }()
	firstEvent := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "claim-first", 1000))
	if firstEvent == nil || firstEvent.SourceID != "oauth-account" {
		t.Fatalf("first event=%+v", firstEvent)
	}

	if immediate := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "claim-immediate", 0)); immediate != nil {
		t.Fatalf("claimed event was redelivered without lease: %+v", immediate)
	}
	rt.mu.Lock()
	pending := rt.oauthPendingByID[firstEvent.RefreshID]
	leaseRemaining := time.Duration(0)
	if pending != nil {
		leaseRemaining = pending.nextDeliveryAt.Sub(rt.oauthTime())
	}
	rt.mu.Unlock()
	if leaseRemaining < time.Second || leaseRemaining > oauthRefreshRedelivery {
		t.Fatalf("redelivery lease=%s", leaseRemaining)
	}

	go func() { secondResult <- rt.requestOAuthRefresh(secondCtx, "oauth-edge", secondMember) }()
	secondEvent := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "claim-second", 1000))
	if secondEvent == nil || secondEvent.SourceID != "oauth-account-2" {
		t.Fatalf("new event was blocked by claimed event: %+v", secondEvent)
	}
	if immediate := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "claim-immediate-again", 0)); immediate != nil {
		t.Fatalf("leased events were redelivered too quickly: %+v", immediate)
	}

	cancelFirst()
	cancelSecond()
	select {
	case <-firstResult:
	case <-time.After(time.Second):
		t.Fatal("first canceled waiter did not finish")
	}
	select {
	case <-secondResult:
	case <-time.After(time.Second):
		t.Fatal("second canceled waiter did not finish")
	}
}

func TestOAuthRefreshAllowsCoreAndReloadSlowCompletionWindow(t *testing.T) {
	if oauthRefreshTTL < 60*time.Second {
		t.Fatalf("OAuth refresh TTL=%s, want at least 60s", oauthRefreshTTL)
	}
	rt, epoch, term, _ := ownedStartedOAuthRuntime(t, oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestOldKey, refreshCodexOAuth), "hash-slow-old")
	base := time.Now()
	rt.oauthNow = func() time.Time { return base }
	member := firstOAuthMember(t, rt)
	result := make(chan *oauthRefreshRetry, 1)
	go func() { result <- rt.requestOAuthRefresh(context.Background(), "oauth-edge", member) }()
	event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "slow-next", 1000))
	if event == nil {
		t.Fatal("missing slow refresh event")
	}

	// Simulate the core HTTP allowance plus a full route reload without making
	// the test sleep for 46 seconds.
	rt.oauthNow = func() time.Time { return base.Add(46 * time.Second) }
	if err := rt.SwapRuntimeConfig(oauthRuntimeConfig("http://127.0.0.1:18080", oauthTestNewKey, refreshCodexOAuth), "hash-slow-new"); err != nil {
		t.Fatal(err)
	}
	reply := completeOAuthEvent(t, rt, epoch, term, "slow-complete", event, "hash-slow-new", oauthRefreshApplied)
	if !reply.OK {
		t.Fatalf("slow completion rejected: %+v", reply.Error)
	}
	select {
	case retry := <-result:
		if retry == nil || retry.member.UpstreamKey != oauthTestNewKey {
			t.Fatal("slow completion did not release refreshed member")
		}
	case <-time.After(time.Second):
		t.Fatal("slow completion did not release waiter")
	}
}

func TestOAuth401RefreshRetriesSameMemberOnceAndCoalescesWaiters(t *testing.T) {
	var oldAttempts atomic.Int64
	var newAttempts atomic.Int64
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.Header.Get("Authorization") {
		case "Bearer " + oauthTestOldKey:
			oldAttempts.Add(1)
			http.Error(w, "expired", http.StatusUnauthorized)
		case "Bearer " + oauthTestNewKey:
			newAttempts.Add(1)
			w.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(w, `{"id":"response_ok","object":"response","status":"completed"}`)
		default:
			http.Error(w, "unexpected", http.StatusForbidden)
		}
	}))
	defer upstream.Close()

	rt, epoch, term, port := ownedStartedOAuthRuntime(t, oauthRuntimeConfig(upstream.URL, oauthTestOldKey, refreshCodexOAuth), "hash-http-old")
	requestBody := []byte(`{"model":"` + oauthTestModel + `","input":"probe"}`)
	type responseResult struct {
		status int
		body   string
		err    error
	}
	const clients = 6
	results := make(chan responseResult, clients)
	var started sync.WaitGroup
	started.Add(clients)
	for i := 0; i < clients; i++ {
		go func() {
			started.Done()
			req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+itoa(port)+"/v1/responses", bytes.NewReader(requestBody))
			if err != nil {
				results <- responseResult{err: err}
				return
			}
			req.Header.Set("Authorization", "Bearer "+oauthTestIngress)
			req.Header.Set("Content-Type", "application/json")
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				results <- responseResult{err: err}
				return
			}
			body, readErr := io.ReadAll(resp.Body)
			_ = resp.Body.Close()
			results <- responseResult{status: resp.StatusCode, body: string(body), err: readErr}
		}()
	}
	started.Wait()

	event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "http-next", 1000))
	if event == nil {
		t.Fatal("missing coalesced refresh event")
	}
	rt.mu.Lock()
	if got := len(rt.oauthPendingByID); got != 1 {
		rt.mu.Unlock()
		t.Fatalf("pending refreshes=%d, want 1", got)
	}
	rt.mu.Unlock()
	if err := rt.SwapRuntimeConfig(oauthRuntimeConfig(upstream.URL, oauthTestNewKey, refreshCodexOAuth), "hash-http-new"); err != nil {
		t.Fatal(err)
	}
	if reply := completeOAuthEvent(t, rt, epoch, term, "http-complete", event, "hash-http-new", oauthRefreshApplied); !reply.OK {
		t.Fatalf("complete: %+v", reply.Error)
	}
	for i := 0; i < clients; i++ {
		select {
		case result := <-results:
			if result.err != nil || result.status != http.StatusOK || !strings.Contains(result.body, "response_ok") {
				t.Fatalf("request result=%+v", result)
			}
		case <-time.After(3 * time.Second):
			t.Fatal("request did not finish after refresh")
		}
	}
	if got := newAttempts.Load(); got != clients {
		t.Fatalf("new-token attempts=%d, want %d", got, clients)
	}
	if got := oldAttempts.Load(); got < 1 || got > clients {
		t.Fatalf("old-token attempts=%d, want 1..%d", got, clients)
	}
}

func TestOAuth401UsesOnlyOneRefreshAndOneSameMemberRetry(t *testing.T) {
	var attempts atomic.Int64
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		attempts.Add(1)
		http.Error(w, "still unauthorized", http.StatusUnauthorized)
	}))
	defer upstream.Close()
	rt, epoch, term, port := ownedStartedOAuthRuntime(t, oauthRuntimeConfig(upstream.URL, oauthTestOldKey, refreshCodexOAuth), "hash-once-old")

	result := make(chan int, 1)
	go func() {
		req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+itoa(port)+"/v1/responses", strings.NewReader(`{"model":"`+oauthTestModel+`"}`))
		if err != nil {
			result <- 0
			return
		}
		req.Header.Set("Authorization", "Bearer "+oauthTestIngress)
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			result <- 0
			return
		}
		_ = resp.Body.Close()
		result <- resp.StatusCode
	}()

	event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "once-next", 1000))
	if event == nil {
		t.Fatal("missing refresh event")
	}
	if err := rt.SwapRuntimeConfig(oauthRuntimeConfig(upstream.URL, oauthTestNewKey, refreshCodexOAuth), "hash-once-new"); err != nil {
		t.Fatal(err)
	}
	if reply := completeOAuthEvent(t, rt, epoch, term, "once-complete", event, "hash-once-new", oauthRefreshApplied); !reply.OK {
		t.Fatalf("complete: %+v", reply.Error)
	}
	select {
	case status := <-result:
		if status != http.StatusUnauthorized {
			t.Fatalf("status=%d", status)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("request did not finish")
	}
	if got := attempts.Load(); got != 2 {
		t.Fatalf("attempts=%d, want exactly 2", got)
	}
	if event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "once-next-again", 0)); event != nil {
		t.Fatalf("second 401 created another refresh: %+v", event)
	}
}

func TestOAuthRefreshDoesNotTriggerFor403OrRefreshNone(t *testing.T) {
	for _, tc := range []struct {
		name        string
		status      int
		refreshKind string
	}{
		{name: "forbidden OAuth", status: http.StatusForbidden, refreshKind: refreshCodexOAuth},
		{name: "unauthorized API Key style", status: http.StatusUnauthorized, refreshKind: refreshNone},
	} {
		t.Run(tc.name, func(t *testing.T) {
			upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
				http.Error(w, "denied", tc.status)
			}))
			defer upstream.Close()
			rt, epoch, term, port := ownedStartedOAuthRuntime(t, oauthRuntimeConfig(upstream.URL, oauthTestOldKey, tc.refreshKind), "hash-no-refresh")
			req, err := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+itoa(port)+"/v1/responses", strings.NewReader(`{"model":"`+oauthTestModel+`"}`))
			if err != nil {
				t.Fatal(err)
			}
			req.Header.Set("Authorization", "Bearer "+oauthTestIngress)
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				t.Fatal(err)
			}
			_ = resp.Body.Close()
			if resp.StatusCode != tc.status {
				t.Fatalf("status=%d, want %d", resp.StatusCode, tc.status)
			}
			if event := decodeOAuthEvent(t, nextOAuthEvent(t, rt, epoch, term, "no-refresh-next", 0)); event != nil {
				t.Fatalf("unexpected refresh event: %+v", event)
			}
		})
	}
}
