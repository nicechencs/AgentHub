package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"strconv"
	"time"
)

const prepareTokenTTL = 10 * time.Minute

type revisionState struct {
	Revision string
	Hash     string
	Snapshot json.RawMessage
}

type preparedState struct {
	Revision           string
	Hash               string
	OperationID        string
	PrepareToken       string
	TokenExpiry        time.Time
	BaseActiveRevision *string
	RequestID          string
	PayloadHash        string
	OwnerTerm          int64
	OwnerID            string
	InstanceEpoch      string
	Snapshot           json.RawMessage
	Kind               string
}

type operationRecord struct {
	OperationID    string
	RequestID      string
	Kind           string
	State          string
	ConfigRevision string
	Hash           string
	StartedAt      time.Time
	FinishedAt     *time.Time
}

func (rt *Runtime) handleBootstrap(env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	req, errReply := rt.parseDesiredRequest(env)
	if errReply != nil {
		return *errReply
	}
	if req.OperationID == "" || req.ConfigRevision == "" {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "operation_id and config_revision are required", false)
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.expirePreparedLocked()

	if req.ExpectedEpoch != "" && req.ExpectedEpoch != rt.instanceEpoch {
		return rt.failUnlocked(env.Type, env.RequestID, errStaleEpoch, "expected_epoch does not match", false)
	}
	if rt.active != nil {
		return rt.failUnlocked(env.Type, env.RequestID, errActiveNotNull, "BootstrapDesired requires a null active revision", false)
	}
	if op := rt.operations[req.OperationID]; op != nil && op.State == opInProgress {
		return rt.failUnlocked(env.Type, env.RequestID, errOperationInProgress, "this operation is still in progress", true)
	}
	if rt.prepared != nil {
		if rt.prepared.OperationID == req.OperationID && rt.prepared.Hash == req.Hash && rt.prepared.Revision == req.ConfigRevision {
			return rt.bootstrapReplyLocked(env, rt.prepared)
		}
		return rt.failUnlocked(env.Type, env.RequestID, errPrepareConflict, "another prepared snapshot is already outstanding", false)
	}

	prepared := rt.newPreparedLocked(env, req, typeBootstrapDesired, nil)
	rt.prepared = prepared
	rt.lifecycle = lifecyclePreparedOnly
	rt.recordOperationLocked(env, req, typeBootstrapDesired, opPrepared)
	rt.logf("bootstrap prepared revision=%s hash=%s op=%s", prepared.Revision, prepared.Hash, prepared.OperationID)
	return rt.bootstrapReplyLocked(env, prepared)
}

func (rt *Runtime) handlePrepare(env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	req, errReply := rt.parseDesiredRequest(env)
	if errReply != nil {
		return *errReply
	}
	if req.OperationID == "" || req.ConfigRevision == "" || req.BaseRevision == "" {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "operation_id, config_revision, and base_revision are required", false)
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.expirePreparedLocked()

	if req.ExpectedEpoch != "" && req.ExpectedEpoch != rt.instanceEpoch {
		return rt.failUnlocked(env.Type, env.RequestID, errStaleEpoch, "expected_epoch does not match", false)
	}
	if rt.active == nil {
		return rt.failUnlocked(env.Type, env.RequestID, errActiveNull, "PrepareDesired requires an active revision", false)
	}
	if req.BaseRevision != rt.active.Revision {
		return rt.failUnlocked(env.Type, env.RequestID, errBaseMismatch, "base_revision does not match active_revision", false)
	}
	if !revisionGreater(req.ConfigRevision, rt.active.Revision) {
		return rt.failUnlocked(env.Type, env.RequestID, errRevisionLow, "config_revision must be strictly greater than active_revision", false)
	}
	if op := rt.operations[req.OperationID]; op != nil && op.State == opInProgress {
		return rt.failUnlocked(env.Type, env.RequestID, errOperationInProgress, "this operation is still in progress", true)
	}
	if rt.prepared != nil {
		if rt.prepared.OperationID == req.OperationID && rt.prepared.Hash == req.Hash && rt.prepared.Revision == req.ConfigRevision {
			base := rt.activeViewLocked()
			return rt.prepareReplyLocked(env, rt.prepared, base)
		}
		return rt.failUnlocked(env.Type, env.RequestID, errPrepareConflict, "another prepared snapshot is already outstanding", false)
	}
	if rt.active.Revision == req.ConfigRevision && rt.active.Hash != req.Hash {
		return rt.failUnlocked(env.Type, env.RequestID, errHashConflict, "same revision with a different hash", false)
	}

	baseRev := rt.active.Revision
	prepared := rt.newPreparedLocked(env, req, typePrepareDesired, &baseRev)
	rt.prepared = prepared
	rt.recordOperationLocked(env, req, typePrepareDesired, opPrepared)
	rt.logf("prepare revision=%s hash=%s op=%s", prepared.Revision, prepared.Hash, prepared.OperationID)
	return rt.prepareReplyLocked(env, prepared, rt.activeViewLocked())
}

func (rt *Runtime) handleCommit(env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	req, errReply := rt.parseDesiredRequest(env)
	if errReply != nil {
		return *errReply
	}
	if req.OperationID == "" || req.PrepareToken == "" {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "operation_id and prepare_token are required", false)
	}

	rt.mu.Lock()
	rt.expirePreparedLocked()
	if op := rt.operations[req.OperationID]; op != nil && op.State == opCommitted && rt.active != nil && rt.active.Revision == req.ConfigRevision {
		success := rt.commitSuccessLocked()
		epoch := rt.instanceEpoch
		rt.mu.Unlock()
		return Reply{OK: true, Type: typeCommitDesired, RequestID: env.RequestID, InstanceEpoch: epoch, Payload: marshalPayload(success)}
	}
	if op := rt.operations[req.OperationID]; op != nil && op.State == opInProgress {
		rt.mu.Unlock()
		return rt.fail(env.Type, env.RequestID, errOperationInProgress, "this operation is still in progress", true)
	}
	if rt.prepared == nil {
		rt.mu.Unlock()
		return rt.fail(env.Type, env.RequestID, errPrepareExpired, "no prepared snapshot to commit", false)
	}
	if rt.prepared.OperationID != req.OperationID || rt.prepared.PrepareToken != req.PrepareToken {
		rt.mu.Unlock()
		return rt.fail(env.Type, env.RequestID, errTokenInvalid, "prepare_token does not match this operation", false)
	}
	if rt.prepared.OwnerTerm != rt.ownerTerm || rt.prepared.OwnerID != rt.ownerID || rt.prepared.InstanceEpoch != rt.instanceEpoch {
		rt.mu.Unlock()
		return rt.fail(env.Type, env.RequestID, errTokenInvalid, "prepare_token does not match the current owner", false)
	}
	if req.ConfigRevision != "" && req.ConfigRevision != rt.prepared.Revision {
		rt.mu.Unlock()
		return rt.fail(env.Type, env.RequestID, errHashConflict, "config_revision does not match prepared", false)
	}
	if req.Hash != "" && req.Hash != rt.prepared.Hash {
		rt.mu.Unlock()
		return rt.fail(env.Type, env.RequestID, errHashConflict, "hash does not match prepared", false)
	}

	rt.recordOperationLocked(env, req, typeCommitDesired, opInProgress)
	rt.active = &revisionState{
		Revision: rt.prepared.Revision,
		Hash:     rt.prepared.Hash,
		Snapshot: rt.prepared.Snapshot,
	}
	rt.prepared = nil
	alreadyListening := rt.listenReady
	epoch := rt.instanceEpoch
	rt.mu.Unlock()

	if !alreadyListening {
		if err := rt.startMessagesFromProbe(); err != nil {
			rt.mu.Lock()
			if rt.lifecycle != lifecycleServing {
				rt.lifecycle = lifecycleNotServing
			}
			rt.recordOperationLocked(env, req, typeCommitDesired, opCommitted)
			now := time.Now().UTC()
			if op := rt.operations[req.OperationID]; op != nil {
				op.FinishedAt = &now
			}
			rt.lastError = &LastError{Code: errPortInUse, Message: "messages listener failed to bind", ObservedAt: now.Format(time.RFC3339)}
			success := rt.commitSuccessLocked()
			rt.mu.Unlock()
			rt.logf("commit revision=%s but messages listen failed", success.Active.Revision)
			return Reply{OK: true, Type: typeCommitDesired, RequestID: env.RequestID, InstanceEpoch: epoch, Payload: marshalPayload(success)}
		}
	}

	rt.mu.Lock()
	rt.recordOperationLocked(env, req, typeCommitDesired, opCommitted)
	now := time.Now().UTC()
	if op := rt.operations[req.OperationID]; op != nil {
		op.FinishedAt = &now
	}
	success := rt.commitSuccessLocked()
	rt.mu.Unlock()
	rt.logf("commit revision=%s hash=%s listen_ready=%v", success.Active.Revision, success.Active.Hash, success.ListenReady)
	return Reply{OK: true, Type: typeCommitDesired, RequestID: env.RequestID, InstanceEpoch: epoch, Payload: marshalPayload(success)}
}

func (rt *Runtime) handleAbort(env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	req, errReply := rt.parseDesiredRequest(env)
	if errReply != nil {
		return *errReply
	}
	if req.OperationID == "" {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "operation_id is required", false)
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.expirePreparedLocked()

	if op := rt.operations[req.OperationID]; op != nil && op.State == opCommitted {
		return rt.failUnlocked(env.Type, env.RequestID, errAlreadyCommitted, "committed operations cannot be aborted", false)
	}
	if rt.prepared != nil && rt.prepared.OperationID == req.OperationID {
		if req.PrepareToken != "" && req.PrepareToken != rt.prepared.PrepareToken {
			return rt.failUnlocked(env.Type, env.RequestID, errTokenInvalid, "prepare_token does not match this operation", false)
		}
		rt.prepared = nil
		if rt.active == nil && rt.lifecycle == lifecyclePreparedOnly {
			rt.lifecycle = lifecycleNotServing
		}
	} else if rt.operations[req.OperationID] == nil {
		return rt.failUnlocked(env.Type, env.RequestID, errOperationUnknown, "no retained operation matches this abort", false)
	}

	state := opAborted
	if op := rt.operations[req.OperationID]; op != nil && op.State == opExpired {
		state = opExpired
	}
	rt.recordOperationLocked(env, req, typeAbortDesired, state)
	now := time.Now().UTC()
	if op := rt.operations[req.OperationID]; op != nil {
		op.FinishedAt = &now
	}
	rt.logf("abort op=%s state=%s", req.OperationID, state)
	return Reply{
		OK:            true,
		Type:          typeAbortDesired,
		RequestID:     env.RequestID,
		InstanceEpoch: rt.instanceEpoch,
		Payload: marshalPayload(AbortSuccess{
			Prepared: nil,
			Active:   rt.activeViewLocked(),
		}),
	}
}

func (rt *Runtime) handleGetOperation(env Envelope) Reply {
	if fail := rt.requireHandshake(env); fail != nil {
		return *fail
	}
	req, errReply := rt.parseDesiredRequest(env)
	if errReply != nil {
		return *errReply
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()
	rt.expirePreparedLocked()

	op := rt.operations[req.OperationID]
	if op == nil && req.RequestID != "" {
		for _, candidate := range rt.operations {
			if candidate.RequestID == req.RequestID {
				op = candidate
				break
			}
		}
	}
	if op == nil && env.RequestID != "" && req.OperationID == "" {
		for _, candidate := range rt.operations {
			if candidate.RequestID == env.RequestID {
				op = candidate
				break
			}
		}
	}
	if op == nil {
		return rt.failUnlocked(env.Type, env.RequestID, errOperationUnknown, "no retained operation matches this query", false)
	}

	port := rt.actualPort
	if port == 0 {
		port = rt.listenPort
	}
	var portPtr *int
	if port > 0 {
		p := port
		portPtr = &p
	}
	success := GetOperationSuccess{
		OperationID:    op.OperationID,
		OperationState: op.State,
		Active:         rt.activeViewLocked(),
		Prepared:       rt.preparedViewLocked(),
		Lifecycle:      rt.lifecycle,
		Port:           portPtr,
		InFlightCount:  rt.inFlight,
	}
	return Reply{
		OK:            true,
		Type:          typeGetOperation,
		RequestID:     env.RequestID,
		InstanceEpoch: rt.instanceEpoch,
		Payload:       marshalPayload(success),
	}
}

func (rt *Runtime) parseDesiredRequest(env Envelope) (DesiredRequest, *Reply) {
	var req DesiredRequest
	if len(env.Payload) > 0 {
		if err := json.Unmarshal(env.Payload, &req); err != nil {
			fail := rt.fail(env.Type, env.RequestID, errInvalidRequest, "desired-config payload is not valid JSON", false)
			return req, &fail
		}
	}
	if controlContainsForbiddenFields(req.Snapshot) {
		fail := rt.fail(env.Type, env.RequestID, errSecretOnControl, "desired snapshot must not include login or entry-key fields", false)
		return req, &fail
	}
	hasSnapshot := len(req.Snapshot) > 0 && string(req.Snapshot) != "null"
	if !hasSnapshot {
		req.Snapshot = json.RawMessage(`{}`)
		return req, nil
	}
	computed := snapshotHash(req.Snapshot)
	if req.Hash == "" {
		req.Hash = computed
	} else if req.Hash != computed {
		fail := rt.fail(env.Type, env.RequestID, errHashConflict, "hash does not match snapshot", false)
		return req, &fail
	}
	return req, nil
}

func (rt *Runtime) newPreparedLocked(env Envelope, req DesiredRequest, kind string, base *string) *preparedState {
	token := newOpaqueID("tok")
	var baseCopy *string
	if base != nil {
		v := *base
		baseCopy = &v
	}
	return &preparedState{
		Revision:           req.ConfigRevision,
		Hash:               req.Hash,
		OperationID:        req.OperationID,
		PrepareToken:       token,
		TokenExpiry:        time.Now().Add(prepareTokenTTL),
		BaseActiveRevision: baseCopy,
		RequestID:          env.RequestID,
		PayloadHash:        env.PayloadHash,
		OwnerTerm:          rt.ownerTerm,
		OwnerID:            rt.ownerID,
		InstanceEpoch:      rt.instanceEpoch,
		Snapshot:           req.Snapshot,
		Kind:               kind,
	}
}

func (rt *Runtime) recordOperationLocked(env Envelope, req DesiredRequest, kind, state string) {
	id := req.OperationID
	if id == "" {
		return
	}
	op := rt.operations[id]
	if op == nil {
		op = &operationRecord{
			OperationID: id,
			StartedAt:   time.Now().UTC(),
		}
		rt.operations[id] = op
	}
	op.RequestID = env.RequestID
	op.Kind = kind
	op.State = state
	if req.ConfigRevision != "" {
		op.ConfigRevision = req.ConfigRevision
	}
	if req.Hash != "" {
		op.Hash = req.Hash
	}
}

func (rt *Runtime) expirePreparedLocked() {
	if rt.prepared == nil {
		return
	}
	if time.Now().Before(rt.prepared.TokenExpiry) {
		return
	}
	if op := rt.operations[rt.prepared.OperationID]; op != nil && op.State == opPrepared {
		op.State = opExpired
		now := time.Now().UTC()
		op.FinishedAt = &now
	}
	rt.prepared = nil
	if rt.active == nil && rt.lifecycle == lifecyclePreparedOnly {
		rt.lifecycle = lifecycleNotServing
	}
}

func (rt *Runtime) preparedViewLocked() *PreparedView {
	if rt.prepared == nil {
		return nil
	}
	return &PreparedView{
		Revision:           rt.prepared.Revision,
		Hash:               rt.prepared.Hash,
		OperationID:        rt.prepared.OperationID,
		TokenExpiry:        rt.prepared.TokenExpiry.UTC().Format(time.RFC3339Nano),
		TokenFingerprint:   tokenFingerprint(rt.prepared.PrepareToken),
		BaseActiveRevision: rt.prepared.BaseActiveRevision,
	}
}

func (rt *Runtime) activeViewLocked() *RevisionView {
	if rt.active == nil {
		return nil
	}
	return &RevisionView{Revision: rt.active.Revision, Hash: rt.active.Hash}
}

func (rt *Runtime) bootstrapReplyLocked(env Envelope, prepared *preparedState) Reply {
	return Reply{
		OK:            true,
		Type:          typeBootstrapDesired,
		RequestID:     env.RequestID,
		InstanceEpoch: rt.instanceEpoch,
		Payload: marshalPayload(BootstrapSuccess{
			OperationID: prepared.OperationID,
			Prepared:    preparedSuccessFrom(prepared),
			Active:      nil,
		}),
	}
}

func (rt *Runtime) prepareReplyLocked(env Envelope, prepared *preparedState, active *RevisionView) Reply {
	return Reply{
		OK:            true,
		Type:          typePrepareDesired,
		RequestID:     env.RequestID,
		InstanceEpoch: rt.instanceEpoch,
		Payload: marshalPayload(PrepareSuccess{
			Prepared: preparedSuccessFrom(prepared),
			Active:   active,
		}),
	}
}

func (rt *Runtime) commitSuccessLocked() CommitSuccess {
	var portPtr *int
	port := rt.actualPort
	if port == 0 {
		port = rt.listenPort
	}
	if port > 0 {
		p := port
		portPtr = &p
	}
	active := RevisionView{}
	if rt.active != nil {
		active = RevisionView{Revision: rt.active.Revision, Hash: rt.active.Hash}
	}
	return CommitSuccess{
		Active:      active,
		Prepared:    nil,
		Lifecycle:   rt.lifecycle,
		ListenReady: rt.listenReady,
		Port:        portPtr,
	}
}

func preparedSuccessFrom(prepared *preparedState) PreparedSuccess {
	return PreparedSuccess{
		Revision:           prepared.Revision,
		Hash:               prepared.Hash,
		OperationID:        prepared.OperationID,
		PrepareToken:       prepared.PrepareToken,
		TokenExpiry:        prepared.TokenExpiry.UTC().Format(time.RFC3339Nano),
		BaseActiveRevision: prepared.BaseActiveRevision,
	}
}

func (rt *Runtime) startMessagesFromProbe() error {
	if !isScratchHome(rt.home) {
		return os.ErrPermission
	}
	raw, err := os.ReadFile(defaultProbeFixture(rt.home))
	if err != nil {
		return err
	}
	var fixture ProbeFixture
	if err := json.Unmarshal(raw, &fixture); err != nil {
		return err
	}
	if fixture.IngressKey == "" || fixture.UpstreamBaseURL == "" {
		return os.ErrInvalid
	}
	if err := loopbackURL(fixture.UpstreamBaseURL); err != nil {
		return err
	}
	return rt.startMessagesLocked(fixture)
}

func snapshotHash(raw json.RawMessage) string {
	if len(raw) == 0 {
		raw = []byte("{}")
	}
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}

func tokenFingerprint(token string) string {
	sum := sha256.Sum256([]byte(token))
	hexed := hex.EncodeToString(sum[:])
	if len(hexed) < tokenFingerprintLen {
		return hexed
	}
	return hexed[:tokenFingerprintLen]
}

func revisionGreater(next, current string) bool {
	ni, nerr := strconv.ParseUint(next, 10, 64)
	ci, cerr := strconv.ParseUint(current, 10, 64)
	if nerr == nil && cerr == nil {
		return ni > ci
	}
	return next > current
}
