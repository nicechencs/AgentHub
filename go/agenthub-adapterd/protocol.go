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
	typeStop                = "Stop"

	lifecycleEmpty      = "empty"
	lifecycleServing    = "serving"
	lifecycleNotServing = "not_serving"
	lifecycleStopped    = "stopped"

	errUnauthenticated   = "route.runtime.unauthenticated"
	errProtocolMismatch  = "route.runtime.protocol_mismatch"
	errConfigMismatch    = "route.runtime.config_format_mismatch"
	errPackageMismatch   = "route.runtime.package_mismatch"
	errScopeMismatch     = "route.runtime.scope_mismatch"
	errStaleEpoch        = "route.runtime.stale_epoch"
	errStaleTerm         = "route.runtime.stale_term"
	errOwnerConflict     = "route.runtime.owner_conflict"
	errNotOwner          = "route.runtime.not_owner"
	errSecretOnControl   = "route.runtime.secret_on_control"
	errInvalidRequest    = "route.runtime.invalid_request"
	errPortInUse         = "route.runtime.port_in_use"
	errProbeOnlyRejected = "route.runtime.probe_only_rejected"

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
