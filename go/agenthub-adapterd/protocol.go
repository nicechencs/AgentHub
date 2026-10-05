package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"strings"
)

// Isolated-slice protocol identifiers. These are not the live product
// RouteRuntimeControl versions and must not be treated as a cutover contract.
const (
	protocolVersion     = "route-runtime.v0-isolated"
	configFormatVersion = "route-config.v1-usage-spool"
	extensionID         = "agenthub.routes"

	typeHandshake            = "Handshake"
	typeAcquireOrRenewOwner  = "AcquireOrRenewOwner"
	typeStatus               = "Status"
	typeStart                = "Start"
	typeActivateProbeListen  = "ActivateProbeListen"
	typeNextOAuthRefresh     = "NextOAuthRefresh"
	typeCompleteOAuthRefresh = "CompleteOAuthRefresh"
	typeStop                 = "Stop"

	lifecycleEmpty      = "empty"
	lifecycleServing    = "serving"
	lifecycleDraining   = "draining"
	lifecycleNotServing = "not_serving"
	lifecycleStopped    = "stopped"

	errUnauthenticated   = "route.runtime.unauthenticated"
	errProtocolMismatch  = "route.runtime.protocol_mismatch"
	errConfigMismatch    = "route.runtime.config_format_mismatch"
	errConfigStream      = "route.runtime.config_stream_unavailable"
	errPackageMismatch   = "route.runtime.package_mismatch"
	errScopeMismatch     = "route.runtime.scope_mismatch"
	errStaleEpoch        = "route.runtime.stale_epoch"
	errStaleTerm         = "route.runtime.stale_term"
	errOwnerConflict     = "route.runtime.owner_conflict"
	errNotOwner          = "route.runtime.not_owner"
	errSecretOnControl   = "route.runtime.secret_on_control"
	errInvalidRequest    = "route.runtime.invalid_request"
	errPortInUse         = "route.runtime.port_in_use"
	errListenerFailed    = "route.runtime.listener_failed"
	errLifecycleConflict = "route.runtime.lifecycle_conflict"
	errProbeOnlyRejected = "route.runtime.probe_only_rejected"

	productDefaultPort = 43121
	maxUnixSocketBytes = 100

	runtimeScopeIsolated = "isolated"
	runtimeScopeProduct  = "product"
)

// packageVersion is replaced by the desktop sidecar build with
// `-X main.packageVersion=<desktop version>`. Standalone probes and ordinary
// `go test` builds deliberately retain the isolated development version.
var packageVersion = "0.0.0-isolated"

var baseHandshakeCapabilities = []string{
	"messages.json",
	"messages.sse",
	"control.handshake",
	"control.status",
	"control.acquire_owner",
	"control.start",
	"control.activate_probe_listen",
	"control.oauth_refresh.v1",
	"config.stdin_stream.atomic",
}

type Envelope struct {
	Type          string          `json:"type"`
	RequestID     string          `json:"request_id"`
	PayloadHash   string          `json:"payload_hash,omitempty"`
	InstanceEpoch string          `json:"instance_epoch,omitempty"`
	OwnerTerm     *int64          `json:"owner_term,omitempty"`
	OwnerID       string          `json:"owner_id,omitempty"`
	AppDataDir    string          `json:"app_data_dir,omitempty"`
	Payload       json.RawMessage `json:"payload,omitempty"`
}

type ErrorBody struct {
	Code          string `json:"code"`
	Message       string `json:"message"`
	Retryable     bool   `json:"retryable"`
	InstanceEpoch string `json:"instance_epoch,omitempty"`
	OwnerTerm     *int64 `json:"owner_term,omitempty"`
}

type Reply struct {
	OK            bool            `json:"ok"`
	Type          string          `json:"type"`
	RequestID     string          `json:"request_id"`
	InstanceEpoch string          `json:"instance_epoch,omitempty"`
	Payload       json.RawMessage `json:"payload,omitempty"`
	Error         *ErrorBody      `json:"error,omitempty"`
}

type HandshakePayload struct {
	ProtocolVersion     string `json:"protocol_version"`
	ConfigFormatVersion string `json:"config_format_version"`
	PackageVersion      string `json:"package_version"`
	AppDataDir          string `json:"app_data_dir"`
}

type HandshakeSuccess struct {
	InstanceID          string   `json:"instance_id"`
	InstanceEpoch       string   `json:"instance_epoch"`
	ProtocolVersion     string   `json:"protocol_version"`
	ConfigFormatVersion string   `json:"config_format_version"`
	PackageVersion      string   `json:"package_version"`
	ExtensionID         string   `json:"extension_id"`
	Capabilities        []string `json:"capabilities"`
	Active              *string  `json:"active"`
	Prepared            *string  `json:"prepared"`
}

type AcquirePayload struct {
	Mode          string `json:"mode"`
	LeaseBudgetMS int64  `json:"lease_budget_ms,omitempty"`
	PreviousTerm  *int64 `json:"previous_term,omitempty"`
}

type AcquireSuccess struct {
	OwnerTerm       int64  `json:"owner_term"`
	OwnerLeaseUntil string `json:"owner_lease_until"`
	Mode            string `json:"mode"`
}

type StatusSuccess struct {
	InstanceID         string          `json:"instance_id"`
	InstanceEpoch      string          `json:"instance_epoch"`
	OwnerTerm          *int64          `json:"owner_term"`
	ActiveRevision     *string         `json:"active_revision"`
	ActiveHash         *string         `json:"active_hash"`
	Prepared           json.RawMessage `json:"prepared"`
	Lifecycle          string          `json:"lifecycle"`
	ListenReady        bool            `json:"listen_ready"`
	Port               *int            `json:"port"`
	InFlightCount      int             `json:"in_flight_count"`
	OwnerLeaseValid    bool            `json:"owner_lease_valid"`
	LastError          *LastError      `json:"last_error"`
	SchedulePolicy     string          `json:"schedule_policy,omitempty"`
	MemberCount        int             `json:"member_count,omitempty"`
	HealthyMemberCount int             `json:"healthy_member_count,omitempty"`
	EdgeStatuses       []EdgeStatus    `json:"edge_statuses,omitempty"`
}

// EdgeStatus is the non-secret, per-pool portion of a Status reply. PoolID is
// the stable RuntimeEdgeConfig.ID assigned by the desktop side; ingress keys,
// upstream identities, request bodies, and upstream messages never leave the
// runtime through this structure.
type EdgeStatus struct {
	PoolID              string  `json:"pool_id"`
	Surface             string  `json:"surface"`
	MemberCount         int     `json:"member_count"`
	HealthyMemberCount  int     `json:"healthy_member_count"`
	InFlightCount       int     `json:"in_flight_count"`
	RequestSuccessCount uint64  `json:"request_success_count"`
	RequestFailureCount uint64  `json:"request_failure_count"`
	LastErrorCode       *string `json:"last_error_code,omitempty"`
}

type LastError struct {
	Code       string `json:"code"`
	Message    string `json:"message"`
	ObservedAt string `json:"observed_at,omitempty"`
}

type ProbeFixture struct {
	IngressKey      string        `json:"ingress_key"`
	UpstreamBaseURL string        `json:"upstream_base_url,omitempty"`
	FixtureModel    string        `json:"fixture_model"`
	SchedulePolicy  string        `json:"schedule_policy,omitempty"`
	Members         []ProbeMember `json:"members,omitempty"`
	Surface         string        `json:"-"`
}

type ProbeMember struct {
	ID                string   `json:"id"`
	TicketID          string   `json:"ticket_id,omitempty"`
	SourceKind        string   `json:"source_kind,omitempty"`
	SourceID          string   `json:"source_id,omitempty"`
	RefreshKind       string   `json:"refresh_kind,omitempty"`
	UpstreamBaseURL   string   `json:"upstream_base_url"`
	UpstreamKey       string   `json:"upstream_key,omitempty"`
	UpstreamAuth      string   `json:"upstream_auth,omitempty"`
	UpstreamTransport string   `json:"-"`
	UpstreamTarget    string   `json:"-"`
	CredentialClass   string   `json:"-"`
	OfficialAccountID string   `json:"-"`
	UpstreamModel     string   `json:"-"`
	Priority          int64    `json:"priority"`
	Position          int64    `json:"position"`
	Models            []string `json:"models,omitempty"`
	QuotaRemainingPct *float64 `json:"quota_remaining_pct,omitempty"`
}

// StartPayload is the product Start input for this isolated slice.
// Secrets stay in probe.json; control JSON must not carry them.
type StartPayload struct {
	ListenPort *int `json:"listen_port,omitempty"`
}

func payloadHash(raw json.RawMessage) string {
	if len(raw) == 0 {
		raw = []byte("{}")
	}
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}

func controlReplayScope(env Envelope) string {
	raw, _ := json.Marshal(struct {
		Type          string `json:"type"`
		InstanceEpoch string `json:"instance_epoch"`
		OwnerTerm     *int64 `json:"owner_term"`
		OwnerID       string `json:"owner_id"`
		AppDataDir    string `json:"app_data_dir"`
		PayloadHash   string `json:"payload_hash"`
	}{
		Type:          env.Type,
		InstanceEpoch: env.InstanceEpoch,
		OwnerTerm:     env.OwnerTerm,
		OwnerID:       env.OwnerID,
		AppDataDir:    env.AppDataDir,
		PayloadHash:   env.PayloadHash,
	})
	sum := sha256.Sum256(raw)
	return hex.EncodeToString(sum[:])
}

func marshalPayload(v any) json.RawMessage {
	b, err := json.Marshal(v)
	if err != nil {
		return json.RawMessage(`{}`)
	}
	return b
}

func controlContainsForbiddenFields(raw []byte) bool {
	lower := strings.ToLower(string(raw))
	needles := []string{
		`"ingress_key"`,
		`"ingress_keys"`,
		`"local_token"`,
		`"api_key"`,
		`"upstream_key"`,
		`"authorization"`,
		`"refresh_token"`,
		`"x-api-key"`,
	}
	for _, n := range needles {
		if strings.Contains(lower, n) {
			return true
		}
	}
	return false
}

func statusContainsSecret(statusJSON []byte, secret string) bool {
	return statusContainsAnySecret(statusJSON, []string{secret})
}

func statusContainsAnySecret(statusJSON []byte, secrets []string) bool {
	body := string(statusJSON)
	for _, secret := range secrets {
		if secret != "" && strings.Contains(body, secret) {
			return true
		}
	}
	return false
}
