package main

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

const (
	runtimeConfigVersion = "route-config.v0-isolated"
	maxRuntimeConfigSize = 8 << 20

	surfaceMessages        = "messages"
	surfaceResponses       = "responses"
	surfaceChatCompletions = "chat_completions"

	authBearer = "bearer"
	authAPIKey = "x_api_key"

	transportAnthropicMessages     = "anthropic_messages"
	transportCodexResponses        = "codex_responses"
	transportGrokResponses         = "grok_responses"
	transportOpenAIChatCompletions = "openai_chat_completions"
)

// RuntimeConfig is supplied once on stdin before the control socket starts.
// It is retained only in memory and is never returned through control replies.
type RuntimeConfig struct {
	Version string              `json:"version"`
	Edges   []RuntimeEdgeConfig `json:"edges"`
}

type RuntimeEdgeConfig struct {
	ID             string                `json:"id"`
	IngressKey     string                `json:"ingress_key"`
	IngressKeys    []string              `json:"ingress_keys,omitempty"`
	Surface        string                `json:"surface"`
	Dialect        string                `json:"dialect"`
	SchedulePolicy string                `json:"schedule_policy"`
	FixtureModel   string                `json:"fixture_model"`
	Members        []RuntimeMemberConfig `json:"members"`
}

type RuntimeMemberConfig struct {
	ID                string   `json:"id"`
	UpstreamBaseURL   string   `json:"upstream_base_url"`
	UpstreamKey       string   `json:"upstream_key"`
	UpstreamAuth      string   `json:"upstream_auth"`
	UpstreamTransport string   `json:"upstream_transport"`
	Priority          int64    `json:"priority"`
	Position          int64    `json:"position"`
	Models            []string `json:"models"`
	QuotaRemainingPct *float64 `json:"quota_remaining_pct,omitempty"`
}

type RuntimeEdge struct {
	ID          string
	IngressKey  string
	IngressKeys []string
	Surface     string
	Dialect     string
	Pool        *Pool
}

func (edge *RuntimeEdge) acceptsIngressKey(candidate string) bool {
	if edge == nil || candidate == "" {
		return false
	}
	if edge.IngressKey == candidate {
		return true
	}
	for _, ingressKey := range edge.IngressKeys {
		if ingressKey == candidate {
			return true
		}
	}
	return false
}

func LoadRuntimeConfig(r io.Reader) (*RuntimeConfig, error) {
	limited := io.LimitReader(r, maxRuntimeConfigSize+1)
	raw, err := io.ReadAll(limited)
	if err != nil {
		return nil, fmt.Errorf("read runtime config: %w", err)
	}
	if len(raw) > maxRuntimeConfigSize {
		return nil, fmt.Errorf("runtime config exceeds %d bytes", maxRuntimeConfigSize)
	}
	decoder := json.NewDecoder(strings.NewReader(string(raw)))
	decoder.DisallowUnknownFields()
	var config RuntimeConfig
	if err := decoder.Decode(&config); err != nil {
		return nil, fmt.Errorf("runtime config is not valid JSON: %w", err)
	}
	if err := ensureJSONEOF(decoder); err != nil {
		return nil, err
	}
	if err := validateRuntimeConfig(&config); err != nil {
		return nil, err
	}
	return &config, nil
}

func ensureJSONEOF(decoder *json.Decoder) error {
	var trailing any
	if err := decoder.Decode(&trailing); err == io.EOF {
		return nil
	} else if err != nil {
		return fmt.Errorf("runtime config has trailing data: %w", err)
	}
	return fmt.Errorf("runtime config must contain exactly one JSON value")
}

func validateRuntimeConfig(config *RuntimeConfig) error {
	if config == nil {
		return fmt.Errorf("runtime config is missing")
	}
	if config.Version != runtimeConfigVersion {
		return fmt.Errorf("runtime config version is not accepted")
	}
	if len(config.Edges) == 0 {
		return fmt.Errorf("runtime config requires at least one edge")
	}
	ids := make(map[string]struct{}, len(config.Edges))
	ingressKeyOwners := make(map[string]int, len(config.Edges))
	for edgeIndex := range config.Edges {
		edge := &config.Edges[edgeIndex]
		edge.ID = strings.TrimSpace(edge.ID)
		edge.Surface = strings.TrimSpace(edge.Surface)
		edge.Dialect = strings.TrimSpace(edge.Dialect)
		edge.SchedulePolicy = strings.TrimSpace(edge.SchedulePolicy)
		edge.FixtureModel = strings.TrimSpace(edge.FixtureModel)
		if edge.ID == "" || edge.IngressKey == "" {
			return fmt.Errorf("runtime edge %d is incomplete", edgeIndex)
		}
		if _, exists := ids[edge.ID]; exists {
			return fmt.Errorf("runtime edge id is duplicated")
		}
		ids[edge.ID] = struct{}{}
		switch edge.Surface {
		case surfaceMessages, surfaceResponses, surfaceChatCompletions:
		default:
			return fmt.Errorf("runtime edge %s has unsupported surface", edge.ID)
		}
		if !dialectMatchesSurface(edge.Dialect, edge.Surface) {
			return fmt.Errorf("runtime edge %s dialect does not match surface", edge.ID)
		}
		acceptedIngressKeys := make([]string, 0, len(edge.IngressKeys)+1)
		acceptedIngressKeys = append(acceptedIngressKeys, edge.IngressKey)
		acceptedIngressKeys = append(acceptedIngressKeys, edge.IngressKeys...)
		deduplicatedAliases := make([]string, 0, len(edge.IngressKeys))
		seenOnEdge := make(map[string]struct{}, len(acceptedIngressKeys))
		for keyIndex, ingressKey := range acceptedIngressKeys {
			if ingressKey == "" {
				return fmt.Errorf("runtime edge %d ingress key %d is empty", edgeIndex, keyIndex)
			}
			if owner, exists := ingressKeyOwners[ingressKey]; exists && owner != edgeIndex {
				return fmt.Errorf("runtime edge ingress key is assigned to multiple edges")
			}
			ingressKeyOwners[ingressKey] = edgeIndex
			if _, duplicate := seenOnEdge[ingressKey]; duplicate {
				continue
			}
			seenOnEdge[ingressKey] = struct{}{}
			if keyIndex > 0 {
				deduplicatedAliases = append(deduplicatedAliases, ingressKey)
			}
		}
		edge.IngressKeys = deduplicatedAliases
		if edge.SchedulePolicy == "" {
			edge.SchedulePolicy = policyPriorityFailover
		}
		if edge.SchedulePolicy != policyPriorityFailover && edge.SchedulePolicy != policyRoundRobin {
			return fmt.Errorf("runtime edge %s has invalid schedule_policy", edge.ID)
		}
		if len(edge.Members) == 0 {
			return fmt.Errorf("runtime edge %s requires members", edge.ID)
		}
		for memberIndex := range edge.Members {
			member := &edge.Members[memberIndex]
			member.ID = strings.TrimSpace(member.ID)
			member.UpstreamBaseURL = strings.TrimSpace(member.UpstreamBaseURL)
			member.UpstreamAuth = strings.TrimSpace(member.UpstreamAuth)
			member.UpstreamTransport = strings.TrimSpace(member.UpstreamTransport)
			if member.ID == "" || member.UpstreamBaseURL == "" || member.UpstreamKey == "" {
				return fmt.Errorf("runtime edge %s member %d is incomplete", edge.ID, memberIndex)
			}
			if member.UpstreamAuth != authBearer && member.UpstreamAuth != authAPIKey {
				return fmt.Errorf("runtime edge %s member %s has unsupported upstream_auth", edge.ID, member.ID)
			}
			if (edge.Surface == surfaceMessages && member.UpstreamAuth != authAPIKey) ||
				(edge.Surface != surfaceMessages && member.UpstreamAuth != authBearer) {
				return fmt.Errorf("runtime edge %s member %s auth does not match surface", edge.ID, member.ID)
			}
			if !transportMatchesSurface(member.UpstreamTransport, edge.Surface) {
				return fmt.Errorf("runtime edge %s member %s transport does not match surface", edge.ID, member.ID)
			}
			if err := validateRuntimeUpstreamURL(member.UpstreamBaseURL, member.UpstreamTransport); err != nil {
				return fmt.Errorf("runtime edge %s member %s upstream is not allowed", edge.ID, member.ID)
			}
			if len(member.Models) == 0 && edge.FixtureModel == "" {
				return fmt.Errorf("runtime edge %s member %s has no models", edge.ID, member.ID)
			}
		}
	}
	return nil
}

func transportMatchesSurface(transport, surface string) bool {
	switch surface {
	case surfaceMessages:
		return transport == transportAnthropicMessages
	case surfaceResponses:
		return transport == transportCodexResponses || transport == transportGrokResponses || transport == transportOpenAIChatCompletions
	case surfaceChatCompletions:
		return transport == transportOpenAIChatCompletions
	default:
		return false
	}
}

func dialectMatchesSurface(dialect, surface string) bool {
	switch surface {
	case surfaceMessages:
		return dialect == "claude"
	case surfaceResponses:
		return dialect == "codex" || dialect == "grok"
	case surfaceChatCompletions:
		return dialect == "kimi" || dialect == "dsh" || dialect == "generic"
	default:
		return false
	}
}

func runtimeEdges(config *RuntimeConfig) ([]*RuntimeEdge, error) {
	if config == nil {
		return nil, fmt.Errorf("runtime config is missing")
	}
	edges := make([]*RuntimeEdge, 0, len(config.Edges))
	for _, edge := range config.Edges {
		members := make([]ProbeMember, 0, len(edge.Members))
		for _, member := range edge.Members {
			members = append(members, ProbeMember{
				ID:                member.ID,
				UpstreamBaseURL:   member.UpstreamBaseURL,
				UpstreamKey:       member.UpstreamKey,
				UpstreamAuth:      member.UpstreamAuth,
				UpstreamTransport: member.UpstreamTransport,
				Priority:          member.Priority,
				Position:          member.Position,
				Models:            append([]string(nil), member.Models...),
				QuotaRemainingPct: member.QuotaRemainingPct,
			})
		}
		pool, err := NewPoolFromFixture(ProbeFixture{
			IngressKey:     edge.IngressKey,
			FixtureModel:   edge.FixtureModel,
			SchedulePolicy: edge.SchedulePolicy,
			Members:        members,
		})
		if err != nil {
			return nil, fmt.Errorf("runtime edge %s pool is invalid", edge.ID)
		}
		edges = append(edges, &RuntimeEdge{
			ID:          edge.ID,
			IngressKey:  edge.IngressKey,
			IngressKeys: append([]string(nil), edge.IngressKeys...),
			Surface:     edge.Surface,
			Dialect:     edge.Dialect,
			Pool:        pool,
		})
	}
	return edges, nil
}
