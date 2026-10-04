package main

import (
	"encoding/json"
	"errors"
)

var errInvalidOfficialPairPayload = errors.New("official Responses pair payload is invalid")

const (
	officialPairNone        = ""
	officialPairCodexToGrok = "codex_ingress_grok_upstream"
	officialPairGrokToCodex = "grok_ingress_codex_upstream"
)

func (edge *RuntimeEdge) officialPairDirection(member *PoolMember) string {
	if edge == nil || member == nil {
		return officialPairNone
	}
	if edge.Dialect == "codex" && member.UpstreamTarget == upstreamTargetGrokXAISubscription && edge.CodexIngressGrokUpstream {
		return officialPairCodexToGrok
	}
	if edge.Dialect == "grok" && member.UpstreamTarget == upstreamTargetCodexChatGPTSubscription && edge.GrokIngressCodexUpstream {
		return officialPairGrokToCodex
	}
	return officialPairNone
}

// prepareCodexIngressGrokRequest adapts a Codex-shaped Responses request for
// the Grok upstream. It intentionally does not apply the Codex allowlist:
// Grok reasoning, cache, include, and hosted-tool fields remain available for
// the Grok request preparation stage.
func prepareCodexIngressGrokRequest(raw []byte) ([]byte, bool, error) {
	body, err := decodeOfficialCodexRequest(raw)
	if err != nil {
		return nil, false, errInvalidOfficialPairPayload
	}
	downstreamStream, _ := body["stream"].(bool)
	foldOfficialCodexSystemItems(body)
	delete(body, "store")
	for _, key := range []string{
		"metadata",
		"max_tokens",
		"service_tier",
		"text",
		"truncation",
		"user",
	} {
		delete(body, key)
	}
	prepared, err := json.Marshal(body)
	if err != nil {
		return nil, false, errInvalidOfficialPairPayload
	}
	return prepared, downstreamStream, nil
}

// prepareGrokIngressCodexRequest delegates to the single official Codex
// request policy. This keeps its allowlist, model handling, forced store:false
// and upstream stream:true behavior authoritative in one place.
func prepareGrokIngressCodexRequest(raw []byte, configuredModel string) ([]byte, bool, error) {
	prepared, downstreamStream, err := prepareOfficialCodexRequest(raw, configuredModel)
	if err != nil {
		return nil, false, errInvalidOfficialPairPayload
	}
	return prepared, downstreamStream, nil
}

// sanitizeGrokOfficialResponseForCodex removes Grok-only identity and session
// data recursively while preserving standard Responses fields.
func sanitizeGrokOfficialResponseForCodex(raw []byte) ([]byte, error) {
	return sanitizeOfficialPairJSON(raw, isGrokOnlyOfficialResponseKey)
}

// sanitizeCodexOfficialResponseForGrok removes Codex-only response metadata
// recursively while preserving standard Responses fields.
func sanitizeCodexOfficialResponseForGrok(raw []byte) ([]byte, error) {
	return sanitizeOfficialPairJSON(raw, isCodexOnlyOfficialResponseKey)
}

// SSE data payloads are Responses JSON objects and use the same recursive
// policy. Keeping event entry points explicit prevents callers from assuming
// that a cross-dialect stream can be byte-relayed.
func sanitizeGrokOfficialSSEEventForCodex(raw []byte) ([]byte, error) {
	return sanitizeGrokOfficialResponseForCodex(raw)
}

func sanitizeCodexOfficialSSEEventForGrok(raw []byte) ([]byte, error) {
	return sanitizeCodexOfficialResponseForGrok(raw)
}

func sanitizeOfficialPairJSON(raw []byte, dropKey func(string) bool) ([]byte, error) {
	body, err := decodeOfficialCodexRequest(raw)
	if err != nil {
		return nil, errInvalidOfficialPairPayload
	}
	sanitizeOfficialPairValue(body, dropKey)
	sanitized, err := json.Marshal(body)
	if err != nil {
		return nil, errInvalidOfficialPairPayload
	}
	return sanitized, nil
}

func sanitizeOfficialPairValue(value any, dropKey func(string) bool) {
	switch value := value.(type) {
	case map[string]any:
		for key, child := range value {
			if dropKey(key) {
				delete(value, key)
				continue
			}
			sanitizeOfficialPairValue(child, dropKey)
		}
	case []any:
		for _, child := range value {
			sanitizeOfficialPairValue(child, dropKey)
		}
	}
}

func isGrokOnlyOfficialResponseKey(key string) bool {
	switch key {
	case "prompt_cache_key",
		"session_id",
		"grok_session_id",
		"x_grok_session_id",
		"x_grok_conv_id",
		"x_grok_req_id",
		"x_grok_agent_id",
		"x_grok_client_version",
		"x_grok_model_override",
		"conv_id",
		"conversation_id",
		"server_side_session":
		return true
	}
	return hasOfficialPairPrefix(key, "x_grok_") || hasOfficialPairPrefix(key, "x-grok-")
}

func isCodexOnlyOfficialResponseKey(key string) bool {
	switch key {
	case "store", "service_tier", "metadata":
		return true
	default:
		return false
	}
}

func hasOfficialPairPrefix(value, prefix string) bool {
	return len(value) >= len(prefix) && value[:len(prefix)] == prefix
}
