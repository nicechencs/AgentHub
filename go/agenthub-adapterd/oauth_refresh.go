package main

import (
	"context"
	"encoding/json"
	"strings"
	"time"
)

const (
	maxPendingOAuthRefresh = 64
	maxOAuthRefreshWait    = 5 * time.Second
	oauthRefreshTTL        = 75 * time.Second
	oauthRefreshRedelivery = 2 * time.Second

	oauthRefreshApplied      = "config_applied"
	oauthRefreshNotRefreshed = "not_refreshed"

	errOAuthRefreshMismatch = "route.runtime.oauth_refresh_mismatch"
	errOAuthRefreshCanceled = "route.runtime.oauth_refresh_canceled"
)

type NextOAuthRefreshPayload struct {
	WaitMS int64 `json:"wait_ms,omitempty"`
}

type OAuthRefreshEvent struct {
	RefreshID     string `json:"refresh_id"`
	InstanceEpoch string `json:"instance_epoch"`
	OwnerTerm     int64  `json:"owner_term"`
	ActiveHash    string `json:"active_hash"`
	EdgeID        string `json:"edge_id"`
	MemberID      string `json:"member_id"`
	SourceKind    string `json:"source_kind"`
	SourceID      string `json:"source_id"`
	RefreshKind   string `json:"refresh_kind"`
}

type NextOAuthRefreshSuccess struct {
	Event *OAuthRefreshEvent `json:"event"`
}

type CompleteOAuthRefreshPayload struct {
	RefreshID         string `json:"refresh_id"`
	ActiveHash        string `json:"active_hash"`
	AppliedActiveHash string `json:"applied_active_hash,omitempty"`
	EdgeID            string `json:"edge_id"`
	MemberID          string `json:"member_id"`
	SourceKind        string `json:"source_kind"`
	SourceID          string `json:"source_id"`
	RefreshKind       string `json:"refresh_kind"`
	Outcome           string `json:"outcome"`
}

type CompleteOAuthRefreshSuccess struct {
	Completed     bool `json:"completed"`
	RetryEligible bool `json:"retry_eligible"`
}

type oauthRefreshResult struct {
	retryEligible bool
	activeHash    string
}

type oauthRefreshPending struct {
	event          OAuthRefreshEvent
	key            string
	oldKey         string
	delivered      bool
	nextDeliveryAt time.Time
	expiresAt      time.Time
	done           chan struct{}
	result         oauthRefreshResult
	completed      bool
}

type oauthRefreshRetry struct {
	pool   *Pool
	member *PoolMember
}

func (rt *Runtime) handleNextOAuthRefresh(ctx context.Context, env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	var payload NextOAuthRefreshPayload
	if len(env.Payload) > 0 {
		if err := json.Unmarshal(env.Payload, &payload); err != nil {
			return rt.fail(env.Type, env.RequestID, errInvalidRequest, "NextOAuthRefresh payload is not valid JSON", false)
		}
	}
	if payload.WaitMS < 0 {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "wait_ms must not be negative", false)
	}
	maxWaitMS := int64(maxOAuthRefreshWait / time.Millisecond)
	if payload.WaitMS > maxWaitMS {
		payload.WaitMS = maxWaitMS
	}
	wait := time.Duration(payload.WaitMS) * time.Millisecond
	event, ownerOK, canceled := rt.nextOAuthRefreshEvent(ctx, env, wait)
	if canceled {
		return rt.fail(env.Type, env.RequestID, errOAuthRefreshCanceled, "NextOAuthRefresh was canceled", true)
	}
	if !ownerOK {
		return rt.fail(env.Type, env.RequestID, errNotOwner, "NextOAuthRefresh requires the current owner", false)
	}
	return Reply{
		OK:            true,
		Type:          env.Type,
		RequestID:     env.RequestID,
		InstanceEpoch: env.InstanceEpoch,
		Payload:       marshalPayload(NextOAuthRefreshSuccess{Event: event}),
	}
}

func (rt *Runtime) nextOAuthRefreshEvent(ctx context.Context, env Envelope, wait time.Duration) (*OAuthRefreshEvent, bool, bool) {
	deadline := time.Now().Add(wait)
	for {
		rt.mu.Lock()
		now := rt.oauthTime()
		rt.cleanupExpiredOAuthRefreshLocked(now)
		if !rt.ownerMatchesLocked(env) {
			rt.mu.Unlock()
			return nil, false, false
		}
		pending, nextDeliveryAt := rt.nextQueuedOAuthRefreshLocked(now)
		if pending != nil {
			pending.delivered = true
			pending.nextDeliveryAt = now.Add(oauthRefreshRedelivery)
			event := pending.event
			rt.mu.Unlock()
			return &event, true, false
		}
		notify := rt.oauthNotify
		rt.mu.Unlock()

		remaining := time.Until(deadline)
		if !nextDeliveryAt.IsZero() {
			untilRedelivery := nextDeliveryAt.Sub(now)
			if untilRedelivery < remaining {
				remaining = untilRedelivery
			}
		}
		if remaining <= 0 {
			return nil, true, false
		}
		timer := time.NewTimer(remaining)
		select {
		case <-ctx.Done():
			if !timer.Stop() {
				select {
				case <-timer.C:
				default:
				}
			}
			return nil, true, true
		case <-notify:
			if !timer.Stop() {
				select {
				case <-timer.C:
				default:
				}
			}
		case <-timer.C:
			// Re-check epoch, owner and lease after the wait before replying.
		}
	}
}

func (rt *Runtime) handleCompleteOAuthRefresh(env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	var payload CompleteOAuthRefreshPayload
	if err := json.Unmarshal(env.Payload, &payload); err != nil {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "CompleteOAuthRefresh payload is not valid JSON", false)
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.cleanupExpiredOAuthRefreshLocked(rt.oauthTime())
	if !rt.ownerMatchesLocked(env) {
		return rt.failUnlocked(env.Type, env.RequestID, errNotOwner, "CompleteOAuthRefresh requires the current owner", false)
	}
	pending := rt.oauthPendingByID[payload.RefreshID]
	if pending == nil || !pending.delivered || !completePayloadMatchesEvent(payload, pending.event) {
		return rt.failUnlocked(env.Type, env.RequestID, errOAuthRefreshMismatch, "OAuth refresh completion does not match a pending request", false)
	}

	retryEligible := false
	switch payload.Outcome {
	case oauthRefreshApplied:
		if payload.AppliedActiveHash == "" || payload.AppliedActiveHash == pending.event.ActiveHash || payload.AppliedActiveHash != rt.configHash {
			return rt.failUnlocked(env.Type, env.RequestID, errOAuthRefreshMismatch, "OAuth refresh completion does not match the active configuration", false)
		}
		current := rt.memberByRefreshEventLocked(&pending.event)
		retryEligible = current != nil && current.UpstreamKey != "" && current.UpstreamKey != pending.oldKey
	case oauthRefreshNotRefreshed:
		if payload.AppliedActiveHash != "" {
			return rt.failUnlocked(env.Type, env.RequestID, errOAuthRefreshMismatch, "OAuth refresh completion has an unexpected active hash", false)
		}
	default:
		return rt.failUnlocked(env.Type, env.RequestID, errInvalidRequest, "OAuth refresh outcome is invalid", false)
	}

	rt.completeOAuthRefreshLocked(pending, oauthRefreshResult{
		retryEligible: retryEligible,
		activeHash:    payload.AppliedActiveHash,
	})
	return Reply{
		OK:            true,
		Type:          env.Type,
		RequestID:     env.RequestID,
		InstanceEpoch: env.InstanceEpoch,
		Payload: marshalPayload(CompleteOAuthRefreshSuccess{
			Completed:     true,
			RetryEligible: retryEligible,
		}),
	}
}

func completePayloadMatchesEvent(payload CompleteOAuthRefreshPayload, event OAuthRefreshEvent) bool {
	return payload.RefreshID != "" && payload.RefreshID == event.RefreshID &&
		payload.ActiveHash == event.ActiveHash && payload.EdgeID == event.EdgeID &&
		payload.MemberID == event.MemberID && payload.SourceKind == event.SourceKind &&
		payload.SourceID == event.SourceID && payload.RefreshKind == event.RefreshKind
}

func (rt *Runtime) requestOAuthRefresh(ctx context.Context, edgeID string, member *PoolMember) *oauthRefreshRetry {
	if ctx == nil || member == nil || member.RefreshKind == refreshNone ||
		member.SourceKind != "account" || member.SourceID == "" {
		return nil
	}

	rt.mu.Lock()
	now := rt.oauthTime()
	rt.cleanupExpiredOAuthRefreshLocked(now)
	if !rt.ownerServingLocked(now) || rt.configHash == "" {
		rt.mu.Unlock()
		return nil
	}
	currentPool, current := rt.poolAndMemberByIdentityLocked(edgeID, member.ID, member.SourceKind, member.SourceID, member.RefreshKind)
	if current == nil {
		rt.mu.Unlock()
		return nil
	}
	if current.UpstreamKey != member.UpstreamKey {
		rt.mu.Unlock()
		return &oauthRefreshRetry{pool: currentPool, member: current}
	}
	key := oauthRefreshSingleflightKey(rt.instanceEpoch, rt.configHash, edgeID, member)
	pending := rt.oauthPendingByKey[key]
	if pending == nil {
		if len(rt.oauthPendingByID) >= maxPendingOAuthRefresh {
			rt.mu.Unlock()
			return nil
		}
		pending = &oauthRefreshPending{
			event: OAuthRefreshEvent{
				RefreshID:     newOpaqueID("refresh"),
				InstanceEpoch: rt.instanceEpoch,
				OwnerTerm:     rt.ownerTerm,
				ActiveHash:    rt.configHash,
				EdgeID:        edgeID,
				MemberID:      member.ID,
				SourceKind:    member.SourceKind,
				SourceID:      member.SourceID,
				RefreshKind:   member.RefreshKind,
			},
			key:       key,
			oldKey:    member.UpstreamKey,
			expiresAt: now.Add(oauthRefreshTTL),
			done:      make(chan struct{}),
		}
		rt.oauthPendingByKey[key] = pending
		rt.oauthPendingByID[pending.event.RefreshID] = pending
		rt.oauthQueue = append(rt.oauthQueue, pending.event.RefreshID)
		select {
		case rt.oauthNotify <- struct{}{}:
		default:
		}
	}
	done := pending.done
	expiresAt := pending.expiresAt
	rt.mu.Unlock()

	timer := time.NewTimer(time.Until(expiresAt))
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return nil
	case <-timer.C:
		rt.mu.Lock()
		rt.cleanupExpiredOAuthRefreshLocked(rt.oauthTime())
		rt.mu.Unlock()
		return nil
	case <-done:
		if !pending.result.retryEligible {
			return nil
		}
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()
	if pending.result.activeHash == "" || rt.configHash != pending.result.activeHash || rt.instanceEpoch != pending.event.InstanceEpoch {
		return nil
	}
	pool, refreshed := rt.poolAndMemberByRefreshEventLocked(&pending.event)
	if refreshed == nil || refreshed.UpstreamKey == "" || refreshed.UpstreamKey == pending.oldKey {
		return nil
	}
	return &oauthRefreshRetry{pool: pool, member: refreshed}
}

func oauthRefreshSingleflightKey(epoch, activeHash, edgeID string, member *PoolMember) string {
	return strings.Join([]string{epoch, activeHash, edgeID, member.ID, member.SourceKind, member.SourceID, member.RefreshKind}, "\x00")
}

func (rt *Runtime) ownerMatchesLocked(env Envelope) bool {
	return rt.handshaked && env.InstanceEpoch == rt.instanceEpoch && env.OwnerTerm != nil &&
		*env.OwnerTerm == rt.ownerTerm && env.OwnerID == rt.ownerID &&
		rt.oauthTime().Before(rt.ownerLeaseUntil)
}

func (rt *Runtime) ownerServingLocked(now time.Time) bool {
	return rt.handshaked && rt.ownerTerm > 0 && now.Before(rt.ownerLeaseUntil) &&
		rt.listenReady && rt.lifecycle == lifecycleServing
}

func (rt *Runtime) nextQueuedOAuthRefreshLocked(now time.Time) (*oauthRefreshPending, time.Time) {
	var nextDeliveryAt time.Time
	queued := len(rt.oauthQueue)
	for index := 0; index < queued; index++ {
		refreshID := rt.oauthQueue[0]
		rt.oauthQueue = rt.oauthQueue[1:]
		pending := rt.oauthPendingByID[refreshID]
		if pending == nil || pending.completed {
			continue
		}
		rt.oauthQueue = append(rt.oauthQueue, refreshID)
		if pending.nextDeliveryAt.IsZero() || !now.Before(pending.nextDeliveryAt) {
			return pending, time.Time{}
		}
		if nextDeliveryAt.IsZero() || pending.nextDeliveryAt.Before(nextDeliveryAt) {
			nextDeliveryAt = pending.nextDeliveryAt
		}
	}
	return nil, nextDeliveryAt
}

func (rt *Runtime) oauthTime() time.Time {
	if rt.oauthNow != nil {
		return rt.oauthNow()
	}
	return time.Now()
}

func (rt *Runtime) memberByIdentityLocked(edgeID, memberID, sourceKind, sourceID, refreshKind string) *PoolMember {
	_, member := rt.poolAndMemberByIdentityLocked(edgeID, memberID, sourceKind, sourceID, refreshKind)
	return member
}

func (rt *Runtime) poolAndMemberByIdentityLocked(edgeID, memberID, sourceKind, sourceID, refreshKind string) (*Pool, *PoolMember) {
	for _, edge := range rt.edges {
		if edge.ID == edgeID {
			return edge.Pool, edge.Pool.MemberByIdentity(memberID, sourceKind, sourceID, refreshKind)
		}
	}
	return nil, nil
}

func (rt *Runtime) memberByRefreshEventLocked(event *OAuthRefreshEvent) *PoolMember {
	_, member := rt.poolAndMemberByRefreshEventLocked(event)
	return member
}

func (rt *Runtime) poolAndMemberByRefreshEventLocked(event *OAuthRefreshEvent) (*Pool, *PoolMember) {
	if event == nil {
		return nil, nil
	}
	for _, edge := range rt.edges {
		if edge.ID == event.EdgeID {
			return edge.Pool, edge.Pool.MemberByIdentity(event.MemberID, event.SourceKind, event.SourceID, event.RefreshKind)
		}
	}
	return nil, nil
}

func (rt *Runtime) completeOAuthRefreshLocked(pending *oauthRefreshPending, result oauthRefreshResult) {
	if pending == nil || pending.completed {
		return
	}
	pending.result = result
	pending.completed = true
	delete(rt.oauthPendingByID, pending.event.RefreshID)
	if rt.oauthPendingByKey[pending.key] == pending {
		delete(rt.oauthPendingByKey, pending.key)
	}
	for index, refreshID := range rt.oauthQueue {
		if refreshID == pending.event.RefreshID {
			rt.oauthQueue = append(rt.oauthQueue[:index], rt.oauthQueue[index+1:]...)
			break
		}
	}
	close(pending.done)
}

func (rt *Runtime) cleanupExpiredOAuthRefreshLocked(now time.Time) {
	for _, pending := range rt.oauthPendingByID {
		if !now.Before(pending.expiresAt) {
			rt.completeOAuthRefreshLocked(pending, oauthRefreshResult{})
		}
	}
}

func (rt *Runtime) cancelUndeliveredOAuthRefreshLocked(previousHash string) {
	if previousHash == "" || previousHash == rt.configHash {
		return
	}
	for _, pending := range rt.oauthPendingByID {
		if !pending.delivered && pending.event.ActiveHash == previousHash {
			rt.completeOAuthRefreshLocked(pending, oauthRefreshResult{})
		}
	}
}

func (rt *Runtime) cancelAllOAuthRefreshLocked() {
	for _, pending := range rt.oauthPendingByID {
		rt.completeOAuthRefreshLocked(pending, oauthRefreshResult{})
	}
	rt.oauthQueue = nil
	select {
	case rt.oauthNotify <- struct{}{}:
	default:
	}
}
