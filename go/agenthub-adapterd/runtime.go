package main

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

type Runtime struct {
	mu          sync.Mutex
	lifecycleMu sync.Mutex

	home          string
	listenHost    string
	listenPort    int
	actualPort    int
	controlSocket string
	pidFile       string
	logFile       string
	logger        *log.Logger

	instanceID    string
	instanceEpoch string
	handshaked    bool

	ownerID         string
	ownerTerm       int64
	ownerLeaseUntil time.Time

	lifecycle   string
	listenReady bool
	inFlight    int
	lastError   *LastError

	probe *ProbeFixture
	pool  *Pool
	edges []*RuntimeEdge

	idempotency map[string]idempotentEntry

	messagesSrv *http.Server
	messagesLn  net.Listener

	cancel context.CancelFunc
}

type idempotentEntry struct {
	hash  string
	reply Reply
}

func NewRuntime(home string, listenPort int, controlSocket string, cancel context.CancelFunc) (*Runtime, error) {
	abs, err := resolveAbsolute(home)
	if err != nil {
		return nil, err
	}
	if isForbiddenUserHome(abs) {
		return nil, fmt.Errorf("refusing real user AGENTHUB_HOME")
	}
	if !isScratchHome(abs) {
		return nil, fmt.Errorf("AGENTHUB_HOME must be an absolute scratch directory under /tmp, /var/tmp, or .tmp/route-runtime-probe")
	}
	if listenPort == productDefaultPort {
		return nil, fmt.Errorf("refusing product default listen port %d", productDefaultPort)
	}
	if listenPort < 0 || listenPort > 65535 {
		return nil, fmt.Errorf("invalid listen port %d", listenPort)
	}
	if err := os.MkdirAll(filepath.Join(abs, "run"), 0o700); err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Join(abs, "config"), 0o700); err != nil {
		return nil, err
	}
	if err := os.MkdirAll(filepath.Join(abs, "logs"), 0o700); err != nil {
		return nil, err
	}
	abs, err = resolveAbsolute(abs)
	if err != nil {
		return nil, err
	}
	if controlSocket == "" {
		controlSocket = defaultControlSocket(abs)
	}
	if !filepath.IsAbs(controlSocket) {
		return nil, fmt.Errorf("control socket must be absolute")
	}
	if err := assertSocketPathLength(controlSocket); err != nil {
		return nil, err
	}
	if !isUnderRoot(controlSocket, abs) && !isScratchHome(filepath.Dir(controlSocket)) {
		return nil, fmt.Errorf("control socket must stay under scratch AGENTHUB_HOME")
	}

	logPath := defaultLogFile(abs)
	lf, err := os.OpenFile(logPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return nil, err
	}
	logger := log.New(lf, "", log.LstdFlags|log.Lmsgprefix)

	rt := &Runtime{
		home:          abs,
		listenHost:    "127.0.0.1",
		listenPort:    listenPort,
		controlSocket: controlSocket,
		pidFile:       defaultPIDFile(abs),
		logFile:       logPath,
		logger:        logger,
		lifecycle:     lifecycleEmpty,
		idempotency:   make(map[string]idempotentEntry),
		cancel:        cancel,
	}
	return rt, nil
}

func (rt *Runtime) Home() string          { return rt.home }
func (rt *Runtime) ControlSocket() string { return rt.controlSocket }
func (rt *Runtime) PIDFile() string       { return rt.pidFile }
func (rt *Runtime) LogFile() string       { return rt.logFile }

func (rt *Runtime) SetRuntimeConfig(config *RuntimeConfig) error {
	if err := validateRuntimeConfig(config); err != nil {
		return err
	}
	edges, err := runtimeEdges(config)
	if err != nil {
		return err
	}
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.listenReady {
		return fmt.Errorf("runtime config cannot change while serving")
	}
	rt.edges = edges
	return nil
}

func (rt *Runtime) WritePID() error {
	pid := fmt.Sprintf("%d\n", os.Getpid())
	return os.WriteFile(rt.pidFile, []byte(pid), 0o600)
}

func (rt *Runtime) logf(format string, args ...any) {
	if rt.logger != nil {
		rt.logger.Printf(format, args...)
	}
	log.Printf(format, args...)
}

func (rt *Runtime) HandleControl(raw []byte) Reply {
	if controlContainsForbiddenFields(raw) {
		return rt.fail("", "", errSecretOnControl, "control message must not include login or entry-key fields", false)
	}
	var env Envelope
	if err := json.Unmarshal(raw, &env); err != nil {
		return rt.fail("", "", errInvalidRequest, "control envelope is not valid JSON", false)
	}
	if env.RequestID == "" {
		return rt.fail(env.Type, "", errInvalidRequest, "request_id is required", false)
	}
	if env.Type == "" {
		return rt.fail("", env.RequestID, errInvalidRequest, "type is required", false)
	}

	wantHash := payloadHash(env.Payload)
	if env.PayloadHash != "" && env.PayloadHash != wantHash {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "payload_hash does not match payload", false)
	}
	if env.PayloadHash == "" {
		env.PayloadHash = wantHash
	}

	rt.mu.Lock()
	if prev, ok := rt.idempotency[env.RequestID]; ok {
		rt.mu.Unlock()
		if prev.hash != env.PayloadHash {
			return rt.fail(env.Type, env.RequestID, errInvalidRequest, "request_id reused with a different payload", false)
		}
		return prev.reply
	}
	rt.mu.Unlock()

	rt.mu.Lock()
	probeKey := ""
	if rt.probe != nil {
		probeKey = rt.probe.IngressKey
	}
	rt.mu.Unlock()
	if probeKey != "" && strings.Contains(string(raw), probeKey) {
		return rt.fail(env.Type, env.RequestID, errSecretOnControl, "control message must not include the entry key", false)
	}

	var reply Reply
	switch env.Type {
	case typeHandshake:
		reply = rt.handleHandshake(env)
	case typeAcquireOrRenewOwner:
		reply = rt.handleAcquire(env)
	case typeStatus:
		reply = rt.handleStatus(env)
	case typeStart:
		reply = rt.handleStart(env)
	case typeActivateProbeListen:
		reply = rt.handleActivateProbe(env)
	case typeStop:
		reply = rt.handleStop(env)
	default:
		reply = rt.fail(env.Type, env.RequestID, errInvalidRequest, "unknown control type", false)
	}

	rt.mu.Lock()
	rt.idempotency[env.RequestID] = idempotentEntry{hash: env.PayloadHash, reply: reply}
	rt.mu.Unlock()
	return reply
}

func (rt *Runtime) fail(typ, requestID, code, message string, retryable bool) Reply {
	rt.mu.Lock()
	epoch := rt.instanceEpoch
	var term *int64
	if rt.ownerTerm > 0 {
		t := rt.ownerTerm
		term = &t
	}
	rt.lastError = &LastError{Code: code, Message: message, ObservedAt: time.Now().UTC().Format(time.RFC3339)}
	rt.mu.Unlock()
	return Reply{
		OK:            false,
		Type:          typ,
		RequestID:     requestID,
		InstanceEpoch: epoch,
		Error: &ErrorBody{
			Code:          code,
			Message:       message,
			Retryable:     retryable,
			InstanceEpoch: epoch,
			OwnerTerm:     term,
		},
	}
}

func (rt *Runtime) handleHandshake(env Envelope) Reply {
	if env.OwnerTerm != nil {
		return rt.fail(env.Type, env.RequestID, errSecretOnControl, "Handshake must not include owner_term", false)
	}
	var payload HandshakePayload
	if len(env.Payload) > 0 {
		if err := json.Unmarshal(env.Payload, &payload); err != nil {
			return rt.fail(env.Type, env.RequestID, errInvalidRequest, "Handshake payload is not valid JSON", false)
		}
	}
	appDir := payload.AppDataDir
	if appDir == "" {
		appDir = env.AppDataDir
	}
	if appDir == "" {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "app_data_dir is required", false)
	}
	resolved, err := resolveAbsolute(appDir)
	if err != nil || !sameDir(resolved, rt.home) {
		return rt.fail(env.Type, env.RequestID, errScopeMismatch, "app_data_dir does not match this runner", false)
	}
	if payload.ProtocolVersion != "" && payload.ProtocolVersion != protocolVersion {
		return rt.fail(env.Type, env.RequestID, errProtocolMismatch, "protocol_version is not accepted", false)
	}
	if payload.ConfigFormatVersion != "" && payload.ConfigFormatVersion != configFormatVersion {
		return rt.fail(env.Type, env.RequestID, errConfigMismatch, "config_format_version is not accepted", false)
	}
	if payload.PackageVersion != "" && payload.PackageVersion != packageVersion {
		return rt.fail(env.Type, env.RequestID, errPackageMismatch, "package_version is not accepted", false)
	}

	rt.mu.Lock()
	if !rt.handshaked {
		rt.instanceID = newOpaqueID("inst")
		rt.instanceEpoch = newOpaqueID("epoch")
		rt.handshaked = true
		rt.lifecycle = lifecycleEmpty
		rt.logf("handshake established instance=%s epoch=%s", rt.instanceID, rt.instanceEpoch)
	}
	success := HandshakeSuccess{
		InstanceID:          rt.instanceID,
		InstanceEpoch:       rt.instanceEpoch,
		ProtocolVersion:     protocolVersion,
		ConfigFormatVersion: configFormatVersion,
		PackageVersion:      packageVersion,
		ExtensionID:         extensionID,
		Capabilities:        handshakeCapabilities,
		Active:              nil,
		Prepared:            nil,
	}
	epoch := rt.instanceEpoch
	rt.mu.Unlock()

	return Reply{
		OK:            true,
		Type:          typeHandshake,
		RequestID:     env.RequestID,
		InstanceEpoch: epoch,
		Payload:       marshalPayload(success),
	}
}

func (rt *Runtime) requireHandshake(env Envelope) *Reply {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if !rt.handshaked {
		fail := rt.failUnlocked(env.Type, env.RequestID, errUnauthenticated, "Handshake has not completed", false)
		return &fail
	}
	if env.InstanceEpoch == "" || env.InstanceEpoch != rt.instanceEpoch {
		fail := rt.failUnlocked(env.Type, env.RequestID, errStaleEpoch, "instance_epoch does not match", false)
		return &fail
	}
	appDir := env.AppDataDir
	if appDir != "" {
		resolved, err := resolveAbsolute(appDir)
		if err != nil || !sameDir(resolved, rt.home) {
			fail := rt.failUnlocked(env.Type, env.RequestID, errScopeMismatch, "app_data_dir does not match this runner", false)
			return &fail
		}
	}
	return nil
}

func (rt *Runtime) failUnlocked(typ, requestID, code, message string, retryable bool) Reply {
	epoch := rt.instanceEpoch
	var term *int64
	if rt.ownerTerm > 0 {
		t := rt.ownerTerm
		term = &t
	}
	rt.lastError = &LastError{Code: code, Message: message, ObservedAt: time.Now().UTC().Format(time.RFC3339)}
	return Reply{
		OK:            false,
		Type:          typ,
		RequestID:     requestID,
		InstanceEpoch: epoch,
		Error: &ErrorBody{
			Code:          code,
			Message:       message,
			Retryable:     retryable,
			InstanceEpoch: epoch,
			OwnerTerm:     term,
		},
	}
}

func (rt *Runtime) handleAcquire(env Envelope) Reply {
	if fail := rt.requireHandshake(env); fail != nil {
		return *fail
	}
	var payload AcquirePayload
	if len(env.Payload) > 0 {
		if err := json.Unmarshal(env.Payload, &payload); err != nil {
			return rt.fail(env.Type, env.RequestID, errInvalidRequest, "AcquireOrRenewOwner payload is not valid JSON", false)
		}
	}
	if payload.Mode == "" {
		payload.Mode = "acquire"
	}
	leaseMS := payload.LeaseBudgetMS
	if leaseMS <= 0 {
		leaseMS = 60_000
	}

	rt.mu.Lock()
	defer rt.mu.Unlock()

	switch payload.Mode {
	case "acquire":
		if env.OwnerTerm != nil {
			return rt.failUnlocked(env.Type, env.RequestID, errInvalidRequest, "acquire must not include owner_term", false)
		}
		if rt.ownerTerm > 0 && time.Now().Before(rt.ownerLeaseUntil) {
			return rt.failUnlocked(env.Type, env.RequestID, errOwnerConflict, "this epoch already has an owner", false)
		}
		if env.OwnerID == "" {
			return rt.failUnlocked(env.Type, env.RequestID, errInvalidRequest, "owner_id is required", false)
		}
		rt.ownerID = env.OwnerID
		rt.ownerTerm = 1
		rt.ownerLeaseUntil = time.Now().Add(time.Duration(leaseMS) * time.Millisecond)
		if rt.lifecycle == lifecycleEmpty {
			rt.lifecycle = lifecycleNotServing
		}
		rt.logf("owner acquired term=%d", rt.ownerTerm)
	case "renew":
		if env.OwnerTerm == nil || *env.OwnerTerm != rt.ownerTerm {
			return rt.failUnlocked(env.Type, env.RequestID, errStaleTerm, "owner_term does not match", false)
		}
		if env.OwnerID != rt.ownerID {
			return rt.failUnlocked(env.Type, env.RequestID, errNotOwner, "owner_id does not match", false)
		}
		if time.Now().After(rt.ownerLeaseUntil) {
			return rt.failUnlocked(env.Type, env.RequestID, errNotOwner, "owner lease has expired", false)
		}
		rt.ownerLeaseUntil = time.Now().Add(time.Duration(leaseMS) * time.Millisecond)
	default:
		return rt.failUnlocked(env.Type, env.RequestID, errInvalidRequest, "unsupported owner mode in this isolated slice", false)
	}

	success := AcquireSuccess{
		OwnerTerm:       rt.ownerTerm,
		OwnerLeaseUntil: rt.ownerLeaseUntil.UTC().Format(time.RFC3339Nano),
		Mode:            payload.Mode,
	}
	return Reply{
		OK:            true,
		Type:          typeAcquireOrRenewOwner,
		RequestID:     env.RequestID,
		InstanceEpoch: rt.instanceEpoch,
		Payload:       marshalPayload(success),
	}
}

func (rt *Runtime) handleStatus(env Envelope) Reply {
	if fail := rt.requireHandshake(env); fail != nil {
		return *fail
	}
	status, err := rt.statusSnapshot()
	if err != nil {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, "status snapshot failed", false)
	}
	body := marshalPayload(status)
	rt.mu.Lock()
	secrets := make([]string, 0, 4)
	if rt.probe != nil && rt.probe.IngressKey != "" {
		secrets = append(secrets, rt.probe.IngressKey)
	}
	if rt.pool != nil {
		secrets = append(secrets, rt.pool.Secrets()...)
	}
	for _, edge := range rt.edges {
		secrets = append(secrets, edge.IngressKey)
		secrets = append(secrets, edge.Pool.Secrets()...)
	}
	rt.mu.Unlock()
	if statusContainsAnySecret(body, secrets) {
		return rt.fail(env.Type, env.RequestID, errSecretOnControl, "status refused because it would include a secret", false)
	}
	rt.mu.Lock()
	epoch := rt.instanceEpoch
	rt.mu.Unlock()
	return Reply{
		OK:            true,
		Type:          typeStatus,
		RequestID:     env.RequestID,
		InstanceEpoch: epoch,
		Payload:       body,
	}
}

func (rt *Runtime) statusSnapshot() (StatusSuccess, error) {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	var term *int64
	if rt.ownerTerm > 0 {
		t := rt.ownerTerm
		term = &t
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
	var schedule string
	var memberCount, healthyCount int
	if len(rt.edges) > 0 {
		for _, edge := range rt.edges {
			snap := edge.Pool.Snapshot(time.Now())
			memberCount += snap.MemberCount
			healthyCount += snap.HealthyMemberCount
			if schedule == "" {
				schedule = snap.SchedulePolicy
			} else if schedule != snap.SchedulePolicy {
				schedule = "mixed"
			}
		}
	} else if rt.pool != nil {
		snap := rt.pool.Snapshot(time.Now())
		schedule = snap.SchedulePolicy
		memberCount = snap.MemberCount
		healthyCount = snap.HealthyMemberCount
	}
	return StatusSuccess{
		InstanceID:         rt.instanceID,
		InstanceEpoch:      rt.instanceEpoch,
		OwnerTerm:          term,
		ActiveRevision:     nil,
		ActiveHash:         nil,
		Prepared:           []byte("null"),
		Lifecycle:          rt.lifecycle,
		ListenReady:        rt.listenReady,
		Port:               portPtr,
		InFlightCount:      rt.inFlight,
		OwnerLeaseValid:    rt.ownerTerm > 0 && time.Now().Before(rt.ownerLeaseUntil),
		LastError:          rt.lastError,
		SchedulePolicy:     schedule,
		MemberCount:        memberCount,
		HealthyMemberCount: healthyCount,
	}, nil
}

func (rt *Runtime) requireOwner(env Envelope) *Reply {
	if fail := rt.requireHandshake(env); fail != nil {
		return fail
	}
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.ownerTerm == 0 || env.OwnerTerm == nil || *env.OwnerTerm != rt.ownerTerm {
		fail := rt.failUnlocked(env.Type, env.RequestID, errNotOwner, env.Type+" requires the current owner_term", false)
		return &fail
	}
	if env.OwnerID != rt.ownerID {
		fail := rt.failUnlocked(env.Type, env.RequestID, errNotOwner, "owner_id does not match", false)
		return &fail
	}
	if time.Now().After(rt.ownerLeaseUntil) {
		fail := rt.failUnlocked(env.Type, env.RequestID, errNotOwner, "owner lease has expired", false)
		return &fail
	}
	return nil
}

func (rt *Runtime) handleStart(env Envelope) Reply {
	rt.lifecycleMu.Lock()
	defer rt.lifecycleMu.Unlock()

	rt.mu.Lock()
	hasRuntimeConfig := len(rt.edges) > 0
	rt.mu.Unlock()
	if hasRuntimeConfig {
		return rt.startFromRuntimeConfig(env)
	}
	return rt.startFromProbeFixture(env, "isolated Start; not default gateway")
}

func (rt *Runtime) handleActivateProbe(env Envelope) Reply {
	rt.lifecycleMu.Lock()
	defer rt.lifecycleMu.Unlock()

	return rt.startFromProbeFixture(env, "probe-only activate; not product Start")
}

func (rt *Runtime) startFromRuntimeConfig(env Envelope) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	if fail := rt.rejectStartWhileStopping(env); fail != nil {
		return *fail
	}
	if err := rt.applyOptionalStartPort(env.Payload); err != nil {
		return rt.fail(env.Type, env.RequestID, errInvalidRequest, err.Error(), false)
	}
	rt.mu.Lock()
	if rt.listenReady {
		port := rt.actualPort
		epoch := rt.instanceEpoch
		rt.mu.Unlock()
		return Reply{
			OK:            true,
			Type:          env.Type,
			RequestID:     env.RequestID,
			InstanceEpoch: epoch,
			Payload:       marshalPayload(map[string]any{"listen_ready": true, "port": port}),
		}
	}
	edges := append([]*RuntimeEdge(nil), rt.edges...)
	rt.mu.Unlock()
	if len(edges) == 0 {
		return rt.fail(env.Type, env.RequestID, errConfigMismatch, "runtime config is missing", false)
	}
	if err := rt.startRuntimeEdgesLocked(edges); err != nil {
		return rt.fail(env.Type, env.RequestID, errPortInUse, "route listener failed to bind", false)
	}
	rt.mu.Lock()
	port := rt.actualPort
	epoch := rt.instanceEpoch
	rt.mu.Unlock()
	rt.logf("route listener ready on 127.0.0.1:%d edges=%d", port, len(edges))
	return Reply{
		OK:            true,
		Type:          env.Type,
		RequestID:     env.RequestID,
		InstanceEpoch: epoch,
		Payload:       marshalPayload(map[string]any{"listen_ready": true, "port": port}),
	}
}

func (rt *Runtime) applyOptionalStartPort(raw json.RawMessage) error {
	if len(raw) == 0 {
		return nil
	}
	var payload StartPayload
	if err := json.Unmarshal(raw, &payload); err != nil {
		return fmt.Errorf("Start payload is not valid JSON")
	}
	if payload.ListenPort == nil {
		return nil
	}
	port := *payload.ListenPort
	if port == productDefaultPort {
		return fmt.Errorf("refusing product default listen port %d", productDefaultPort)
	}
	if port < 0 || port > 65535 {
		return fmt.Errorf("invalid listen port %d", port)
	}
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.listenReady {
		return nil
	}
	rt.listenPort = port
	return nil
}

func (rt *Runtime) startFromProbeFixture(env Envelope, note string) Reply {
	if fail := rt.requireOwner(env); fail != nil {
		return *fail
	}
	if fail := rt.rejectStartWhileStopping(env); fail != nil {
		return *fail
	}
	if env.Type == typeStart {
		if err := rt.applyOptionalStartPort(env.Payload); err != nil {
			return rt.fail(env.Type, env.RequestID, errInvalidRequest, err.Error(), false)
		}
	}
	rt.mu.Lock()
	if rt.listenReady {
		port := rt.actualPort
		epoch := rt.instanceEpoch
		rt.mu.Unlock()
		return Reply{
			OK:            true,
			Type:          env.Type,
			RequestID:     env.RequestID,
			InstanceEpoch: epoch,
			Payload: marshalPayload(map[string]any{
				"listen_ready": true,
				"port":         port,
				"note":         note,
			}),
		}
	}
	rt.mu.Unlock()

	if !isScratchHome(rt.home) {
		return rt.fail(env.Type, env.RequestID, errProbeOnlyRejected, env.Type+" is scratch-only", false)
	}
	fixturePath := defaultProbeFixture(rt.home)
	raw, err := os.ReadFile(fixturePath)
	if err != nil {
		return rt.fail(env.Type, env.RequestID, errConfigMismatch, "probe fixture is missing", false)
	}
	var fixture ProbeFixture
	if err := json.Unmarshal(raw, &fixture); err != nil {
		return rt.fail(env.Type, env.RequestID, errConfigMismatch, "probe fixture is not valid JSON", false)
	}
	if fixture.IngressKey == "" || (strings.TrimSpace(fixture.UpstreamBaseURL) == "" && len(fixture.Members) == 0) {
		return rt.fail(env.Type, env.RequestID, errConfigMismatch, "probe fixture is incomplete", false)
	}
	if strings.TrimSpace(fixture.UpstreamBaseURL) != "" {
		if err := loopbackURL(fixture.UpstreamBaseURL); err != nil {
			return rt.fail(env.Type, env.RequestID, errScopeMismatch, "probe upstream must be loopback", false)
		}
	}

	if err := rt.startMessagesLocked(fixture); err != nil {
		switch {
		case err == errIncompletePool || err == errInvalidPolicy:
			return rt.fail(env.Type, env.RequestID, errConfigMismatch, "probe fixture is incomplete", false)
		case strings.Contains(err.Error(), "loopback"):
			return rt.fail(env.Type, env.RequestID, errScopeMismatch, "probe upstream must be loopback", false)
		default:
			return rt.fail(env.Type, env.RequestID, errPortInUse, "messages listener failed to bind", false)
		}
	}
	rt.mu.Lock()
	port := rt.actualPort
	epoch := rt.instanceEpoch
	rt.mu.Unlock()
	rt.logf("messages listening on 127.0.0.1:%d (%s)", port, note)
	return Reply{
		OK:            true,
		Type:          env.Type,
		RequestID:     env.RequestID,
		InstanceEpoch: epoch,
		Payload: marshalPayload(map[string]any{
			"listen_ready": true,
			"port":         port,
			"note":         note,
		}),
	}
}

func (rt *Runtime) rejectStartWhileStopping(env Envelope) *Reply {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.lifecycle != lifecycleDraining && rt.lifecycle != lifecycleStopped {
		return nil
	}
	fail := rt.failUnlocked(env.Type, env.RequestID, errLifecycleConflict, "route runtime is stopping", false)
	return &fail
}

func (rt *Runtime) startMessagesLocked(fixture ProbeFixture) error {
	pool, err := NewPoolFromFixture(fixture)
	if err != nil {
		return err
	}
	rt.mu.Lock()
	rt.probe = &fixture
	rt.pool = pool
	rt.edges = nil
	rt.mu.Unlock()
	return rt.startHTTPServer()
}

func (rt *Runtime) startRuntimeEdgesLocked(edges []*RuntimeEdge) error {
	rt.mu.Lock()
	rt.probe = nil
	rt.pool = nil
	rt.edges = edges
	rt.mu.Unlock()
	return rt.startHTTPServer()
}

func (rt *Runtime) startHTTPServer() error {
	rt.mu.Lock()
	host := rt.listenHost
	port := rt.listenPort
	rt.mu.Unlock()

	addr := fmt.Sprintf("%s:%d", host, port)
	ln, err := net.Listen("tcp", addr)
	if err != nil {
		return err
	}
	tcpAddr, ok := ln.Addr().(*net.TCPAddr)
	if !ok || tcpAddr.IP == nil || !tcpAddr.IP.IsLoopback() {
		_ = ln.Close()
		return fmt.Errorf("messages listener is not loopback")
	}
	srv := &http.Server{
		Handler:           rt.messagesMux(),
		ReadHeaderTimeout: 10 * time.Second,
	}
	rt.mu.Lock()
	rt.messagesLn = ln
	rt.messagesSrv = srv
	rt.actualPort = tcpAddr.Port
	rt.listenReady = true
	rt.lifecycle = lifecycleServing
	rt.mu.Unlock()
	go func() {
		if err := srv.Serve(ln); err != nil && err != http.ErrServerClosed {
			rt.recordServeFailure()
		}
	}()
	return nil
}

func (rt *Runtime) recordServeFailure() {
	rt.mu.Lock()
	if rt.lifecycle == lifecycleDraining || rt.lifecycle == lifecycleStopped {
		rt.mu.Unlock()
		return
	}
	rt.listenReady = false
	rt.lifecycle = lifecycleNotServing
	rt.lastError = &LastError{
		Code:       errListenerFailed,
		Message:    "route listener stopped unexpectedly",
		ObservedAt: time.Now().UTC().Format(time.RFC3339),
	}
	cancel := rt.cancel
	rt.mu.Unlock()
	rt.logf("route listener stopped unexpectedly")
	if cancel != nil {
		cancel()
	}
}

func (rt *Runtime) handleStop(env Envelope) Reply {
	rt.lifecycleMu.Lock()
	defer rt.lifecycleMu.Unlock()

	if fail := rt.requireHandshake(env); fail != nil {
		return *fail
	}
	rt.mu.Lock()
	if rt.ownerTerm > 0 {
		if env.OwnerTerm == nil || *env.OwnerTerm != rt.ownerTerm || env.OwnerID != rt.ownerID {
			rt.mu.Unlock()
			return rt.fail(env.Type, env.RequestID, errNotOwner, "Stop requires the current owner", false)
		}
	}
	epoch := rt.instanceEpoch
	if rt.lifecycle == lifecycleDraining || rt.lifecycle == lifecycleStopped {
		lifecycle := rt.lifecycle
		rt.mu.Unlock()
		return Reply{
			OK:            true,
			Type:          typeStop,
			RequestID:     env.RequestID,
			InstanceEpoch: epoch,
			Payload:       marshalPayload(map[string]any{"lifecycle": lifecycle}),
		}
	}
	cancel := rt.cancel
	rt.lifecycle = lifecycleDraining
	rt.listenReady = false
	rt.mu.Unlock()
	rt.logf("stop requested; shutting down")
	go func() {
		time.Sleep(50 * time.Millisecond)
		shutdownCtx, shutdownCancel := context.WithTimeout(context.Background(), 8*time.Second)
		_ = rt.Shutdown(shutdownCtx)
		shutdownCancel()
		if cancel != nil {
			cancel()
		}
	}()
	return Reply{
		OK:            true,
		Type:          typeStop,
		RequestID:     env.RequestID,
		InstanceEpoch: epoch,
		Payload:       marshalPayload(map[string]any{"lifecycle": lifecycleDraining}),
	}
}

func (rt *Runtime) Shutdown(ctx context.Context) error {
	rt.mu.Lock()
	srv := rt.messagesSrv
	ln := rt.messagesLn
	rt.listenReady = false
	rt.mu.Unlock()
	var shutdownErr error
	if srv != nil {
		shutdownErr = srv.Shutdown(ctx)
		if shutdownErr != nil {
			_ = srv.Close()
		}
	}
	if ln != nil {
		_ = ln.Close()
	}
	rt.mu.Lock()
	rt.lifecycle = lifecycleStopped
	rt.mu.Unlock()
	return shutdownErr
}

func (rt *Runtime) ownerServing() bool {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	return rt.listenReady &&
		rt.lifecycle == lifecycleServing &&
		rt.ownerTerm > 0 &&
		time.Now().Before(rt.ownerLeaseUntil) &&
		(rt.probe != nil || len(rt.edges) > 0)
}

func (rt *Runtime) edgeForRequest(ingressKey, surface string) *RuntimeEdge {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	for _, edge := range rt.edges {
		if edge.IngressKey == ingressKey && edge.Surface == surface {
			return edge
		}
	}
	if rt.probe != nil && rt.probe.IngressKey == ingressKey {
		return &RuntimeEdge{ID: "probe", IngressKey: ingressKey, Surface: surface, Pool: rt.pool}
	}
	return nil
}

func (rt *Runtime) edgeForIngress(ingressKey string) *RuntimeEdge {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	for _, edge := range rt.edges {
		if edge.IngressKey == ingressKey {
			return edge
		}
	}
	if rt.probe != nil && rt.probe.IngressKey == ingressKey {
		return &RuntimeEdge{ID: "probe", IngressKey: ingressKey, Pool: rt.pool}
	}
	return nil
}

func (rt *Runtime) ingressKey() string {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.probe == nil {
		return ""
	}
	return rt.probe.IngressKey
}

func (rt *Runtime) currentPool() *Pool {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	return rt.pool
}

func (rt *Runtime) upstreamBase() string {
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.probe == nil {
		return ""
	}
	return rt.probe.UpstreamBaseURL
}

func (rt *Runtime) addInFlight(delta int) {
	rt.mu.Lock()
	rt.inFlight += delta
	if rt.inFlight < 0 {
		rt.inFlight = 0
	}
	rt.mu.Unlock()
}

func sameDir(a, b string) bool {
	return filepath.Clean(a) == filepath.Clean(b)
}

func newOpaqueID(prefix string) string {
	var b [8]byte
	if _, err := rand.Read(b[:]); err != nil {
		return fmt.Sprintf("%s-%d", prefix, time.Now().UnixNano())
	}
	return prefix + "-" + hex.EncodeToString(b[:])
}
