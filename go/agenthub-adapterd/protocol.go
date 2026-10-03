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
	configFormatVersion = "route-config.v0-isolated"
	packageVersion      = "0.0.0-isolated"
	extensionID         = "agenthub.routes"

	typeHandshake           = "Handshake"
	typeAcquireOrRenewOwner = "AcquireOrRenewOwner"
	typeStatus              = "Status"
	typeActivateProbeListen = "ActivateProbeListen"
	typeBootstrapDesired    = "BootstrapDesired"
	typePrepareDesired      = "PrepareDesired"
	typeCommitDesired       = "CommitDesired"
	typeAbortDesired        = "AbortDesired"
	typeGetOperation        = "GetOperation"
	typeStop                = "Stop"

	lifecycleEmpty        = "empty"
	lifecyclePreparedOnly = "prepared_only"
	lifecycleServing      = "serving"
	lifecycleNotServing   = "not_serving"
	lifecycleStopped      = "stopped"

	opInProgress = "in_progress"
	opPrepared   = "prepared"
	opCommitted  = "committed"
	opAborted    = "aborted"
	opExpired    = "expired"
	opUnknown    = "unknown"

	errUnauthenticated     = "route.runtime.unauthenticated"
	errProtocolMismatch    = "route.runtime.protocol_mismatch"
	errConfigMismatch      = "route.runtime.config_format_mismatch"
	errPackageMismatch     = "route.runtime.package_mismatch"
	errScopeMismatch       = "route.runtime.scope_mismatch"
	errStaleEpoch          = "route.runtime.stale_epoch"
	errStaleTerm           = "route.runtime.stale_term"
	errOwnerConflict       = "route.runtime.owner_conflict"
	errNotOwner            = "route.runtime.not_owner"
	errSecretOnControl     = "route.runtime.secret_on_control"
	errInvalidRequest      = "route.runtime.invalid_request"
	errPortInUse           = "route.runtime.port_in_use"
	errProbeOnlyRejected   = "route.runtime.probe_only_rejected"
	errRevisionLow         = "route.runtime.revision_low"
	errHashConflict        = "route.runtime.hash_conflict"
	errBaseMismatch        = "route.runtime.base_mismatch"
	errActiveNotNull       = "route.runtime.active_not_null"
	errActiveNull          = "route.runtime.active_null"
	errPrepareConflict     = "route.runtime.prepare_conflict"
	errPrepareExpired      = "route.runtime.prepare_expired"
	errTokenInvalid        = "route.runtime.token_invalid"
	errOperationInProgress = "route.runtime.operation_in_progress"
	errOperationUnknown    = "route.runtime.operation_unknown"
	errAlreadyCommitted    = "route.runtime.already_committed"

	tokenFingerprintLen = 12

	productDefaultPort = 43121
	maxUnixSocketBytes = 100
)

var handshakeCapabilities = []string{
	"messages.json",
	"messages.sse",
	"control.handshake",
	"control.status",
	"control.acquire_owner",
	"control.activate_probe_listen",
	"control.bootstrap_desired",
	"control.prepare_desired",
	"control.commit_desired",
	"control.abort_desired",
	"control.get_operation",
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
	InstanceID      string          `json:"instance_id"`
	InstanceEpoch   string          `json:"instance_epoch"`
	OwnerTerm       *int64          `json:"owner_term"`
	ActiveRevision  *string         `json:"active_revision"`
	ActiveHash      *string         `json:"active_hash"`
	Prepared        json.RawMessage `json:"prepared"`
	Lifecycle       string          `json:"lifecycle"`
	ListenReady     bool            `json:"listen_ready"`
	Port            *int            `json:"port"`
	InFlightCount   int             `json:"in_flight_count"`
	OwnerLeaseValid bool            `json:"owner_lease_valid"`
	LastError       *LastError      `json:"last_error"`
}

type LastError struct {
	Code       string `json:"code"`
	Message    string `json:"message"`
	ObservedAt string `json:"observed_at,omitempty"`
}

type ProbeFixture struct {
	IngressKey      string `json:"ingress_key"`
	UpstreamBaseURL string `json:"upstream_base_url"`
	FixtureModel    string `json:"fixture_model"`
}

type DesiredRequest struct {
	OperationID    string          `json:"operation_id"`
	Snapshot       json.RawMessage `json:"snapshot"`
	ConfigRevision string          `json:"config_revision"`
	Hash           string          `json:"hash"`
	ExpectedEpoch  string          `json:"expected_epoch"`
	BaseRevision   string          `json:"base_revision"`
	PrepareToken   string          `json:"prepare_token"`
	RequestID      string          `json:"request_id"`
}

type RevisionView struct {
	Revision string `json:"revision"`
	Hash     string `json:"hash"`
}

type PreparedSuccess struct {
	Revision           string  `json:"revision"`
	Hash               string  `json:"hash"`
	OperationID        string  `json:"operation_id"`
	PrepareToken       string  `json:"prepare_token,omitempty"`
	TokenExpiry        string  `json:"token_expiry"`
	BaseActiveRevision *string `json:"base_active_revision"`
}

type PreparedView struct {
	Revision           string  `json:"revision"`
	Hash               string  `json:"hash"`
	OperationID        string  `json:"operation_id"`
	TokenExpiry        string  `json:"token_expiry"`
	TokenFingerprint   string  `json:"token_fingerprint,omitempty"`
	BaseActiveRevision *string `json:"base_active_revision"`
}

type BootstrapSuccess struct {
	OperationID string          `json:"operation_id"`
	Prepared    PreparedSuccess `json:"prepared"`
	Active      *RevisionView   `json:"active"`
}

type PrepareSuccess struct {
	Prepared PreparedSuccess `json:"prepared"`
	Active   *RevisionView   `json:"active"`
}

type CommitSuccess struct {
	Active      RevisionView  `json:"active"`
	Prepared    *PreparedView `json:"prepared"`
	Lifecycle   string        `json:"lifecycle"`
	ListenReady bool          `json:"listen_ready"`
	Port        *int          `json:"port,omitempty"`
}

type AbortSuccess struct {
	Prepared *PreparedView `json:"prepared"`
	Active   *RevisionView `json:"active"`
}

type GetOperationSuccess struct {
	OperationID    string        `json:"operation_id"`
	OperationState string        `json:"operation_state"`
	Active         *RevisionView `json:"active"`
	Prepared       *PreparedView `json:"prepared"`
	Lifecycle      string        `json:"lifecycle"`
	Port           *int          `json:"port"`
	InFlightCount  int           `json:"in_flight_count"`
}

func payloadHash(raw json.RawMessage) string {
	if len(raw) == 0 {
		raw = []byte("{}")
	}
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
		`"local_token"`,
		`"api_key"`,
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
	if secret == "" {
		return false
	}
	return strings.Contains(string(statusJSON), secret)
}
