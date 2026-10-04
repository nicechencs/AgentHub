package main

import (
	"context"
	"io"
	"net/http"
	"strings"
	"sync/atomic"
	"testing"
)

func firstPolicySurface(policy upstreamTargetPolicy) string {
	for surface := range policy.Surfaces {
		return surface
	}
	return ""
}

func TestExternalTargetPolicyBuildsOnlyFixedFinalURLs(t *testing.T) {
	wants := map[string]string{
		upstreamTargetAnthropicAPI:       "https://api.anthropic.com/v1/messages",
		upstreamTargetOpenAIAPI:          "https://api.openai.com/v1/chat/completions",
		upstreamTargetKimiCodeMembership: "https://api.kimi.com/coding/v1/chat/completions",
	}
	for target, want := range wants {
		policy := upstreamTargetPolicies[target]
		base := "https://" + policy.Host + policy.BasePath
		got, err := buildFinalUpstreamURL(base, policy.LocalEndpoint, policy.Transport, policy.Auth, target, policy.CredentialClass)
		if err != nil {
			t.Fatalf("target %s: %v", target, err)
		}
		if got != want {
			t.Fatalf("target %s final=%q want=%q", target, got, want)
		}
		if _, err := buildFinalUpstreamURL(base, "/v1/not-the-target-path", policy.Transport, policy.Auth, target, policy.CredentialClass); err == nil {
			t.Errorf("target %s accepted a mismatched endpoint", target)
		}
	}
}

func TestExternalTargetPolicyRejectsAdversarialURLs(t *testing.T) {
	for target, policy := range upstreamTargetPolicies {
		if !policy.ExternalAllowed {
			continue
		}
		valid := "https://" + policy.Host + policy.BasePath
		malicious := []string{
			"http://" + policy.Host + policy.BasePath,
			"https://" + policy.Host + ".evil.example" + policy.BasePath,
			"https://" + policy.Host + "@evil.example" + policy.BasePath,
			"https://evil@" + policy.Host + policy.BasePath,
			"https://@" + policy.Host + policy.BasePath,
			"https://" + policy.Host + "." + policy.BasePath,
			"https://" + policy.Host + ":444" + policy.BasePath,
			"https://" + policy.Host + ":0443" + policy.BasePath,
			"https://" + policy.Host + ":" + policy.BasePath,
			valid + "/",
			"https://" + policy.Host + "//" + strings.TrimLeft(policy.BasePath, "/"),
			"https://" + policy.Host + "/../" + strings.TrimLeft(policy.BasePath, "/"),
			"https://" + policy.Host + "/%2e%2e/" + strings.TrimLeft(policy.BasePath, "/"),
			valid + "%2f",
			valid + "?key=value",
			valid + "?",
			valid + "#fragment",
			valid + "#",
			"https://127.0.0.1" + policy.BasePath,
			"https://2130706433" + policy.BasePath,
			"https://127.0.0.1.nip.io" + policy.BasePath,
			"https://" + policy.Host + "\\@evil.example" + policy.BasePath,
			"https://" + policy.Host + policy.BasePath + "\r\nX-Evil: yes",
			"https:" + policy.Host + policy.BasePath,
			"//" + policy.Host + policy.BasePath,
		}
		for _, raw := range malicious {
			if err := validateRuntimeUpstreamURL(raw, firstPolicySurface(policy), policy.Transport, policy.Auth, target, policy.CredentialClass); err == nil {
				t.Errorf("target %s accepted malicious URL %q", target, raw)
			}
		}
	}
}

func TestExternalTargetPolicyRejectsEveryCrossTargetBinding(t *testing.T) {
	allowedTargets := []string{upstreamTargetAnthropicAPI, upstreamTargetOpenAIAPI, upstreamTargetKimiCodeMembership}
	for _, baseTarget := range allowedTargets {
		basePolicy := upstreamTargetPolicies[baseTarget]
		base := "https://" + basePolicy.Host + basePolicy.BasePath
		for target, identityPolicy := range upstreamTargetPolicies {
			if target == baseTarget {
				continue
			}
			if err := validateRuntimeUpstreamURL(
				base,
				firstPolicySurface(identityPolicy),
				identityPolicy.Transport,
				identityPolicy.Auth,
				target,
				identityPolicy.CredentialClass,
			); err == nil {
				t.Errorf("base target %s accepted identity target %s", baseTarget, target)
			}
		}

		wrongSurface := surfaceMessages
		wrongTransport := transportAnthropicMessages
		wrongAuth := authAPIKey
		if baseTarget == upstreamTargetAnthropicAPI {
			wrongSurface = surfaceResponses
			wrongTransport = transportOpenAIChatCompletions
			wrongAuth = authBearer
		}
		mutations := []struct {
			surface, transport, auth, credentialClass string
		}{
			{wrongSurface, basePolicy.Transport, basePolicy.Auth, basePolicy.CredentialClass},
			{firstPolicySurface(basePolicy), wrongTransport, basePolicy.Auth, basePolicy.CredentialClass},
			{firstPolicySurface(basePolicy), basePolicy.Transport, wrongAuth, basePolicy.CredentialClass},
			{firstPolicySurface(basePolicy), basePolicy.Transport, basePolicy.Auth, credentialClassOfficialLogin},
			{firstPolicySurface(basePolicy), basePolicy.Transport, basePolicy.Auth, credentialClassLocal},
		}
		for _, mutation := range mutations {
			if err := validateRuntimeUpstreamURL(
				base,
				mutation.surface,
				mutation.transport,
				mutation.auth,
				baseTarget,
				mutation.credentialClass,
			); err == nil {
				t.Errorf("target %s accepted tuple mutation %+v", baseTarget, mutation)
			}
		}
	}
}

func TestOfficialLoginExternalTargetsUseOnlyExactFinalURL(t *testing.T) {
	for _, target := range []string{upstreamTargetCodexChatGPTSubscription, upstreamTargetGrokXAISubscription} {
		policy := upstreamTargetPolicies[target]
		base := "https://" + policy.Host + policy.BasePath
		if err := validateRuntimeUpstreamURL(base, firstPolicySurface(policy), policy.Transport, policy.Auth, target, policy.CredentialClass); err != nil {
			t.Errorf("official-login target %s rejected: %v", target, err)
		}
		final, err := buildFinalUpstreamURL(base, policy.LocalEndpoint, policy.Transport, policy.Auth, target, policy.CredentialClass)
		if err != nil {
			t.Errorf("official-login target %s failed to build: %v", target, err)
		} else if want := "https://" + policy.Host + policy.FinalPath; final != want {
			t.Errorf("official-login target %s URL=%q want=%q", target, final, want)
		}
	}
}

func TestOfficialAPIKeyTargetsUseOnlyTheirFixedAuthAndURL(t *testing.T) {
	for _, target := range []string{upstreamTargetAnthropicAPI, upstreamTargetOpenAIAPI, upstreamTargetKimiCodeMembership} {
		policy := upstreamTargetPolicies[target]
		base := "https://" + policy.Host + policy.BasePath
		secret := "synthetic-" + target + "-key"
		client := &http.Client{Transport: roundTripFunc(func(request *http.Request) (*http.Response, error) {
			if got, want := request.URL.String(), "https://"+policy.Host+policy.FinalPath; got != want {
				t.Errorf("target %s URL=%q want=%q", target, got, want)
			}
			if request.Host != request.URL.Host {
				t.Errorf("target %s Host=%q URL host=%q", target, request.Host, request.URL.Host)
			}
			if target == upstreamTargetAnthropicAPI {
				if request.Header.Get("X-API-Key") != secret || request.Header.Get("Anthropic-Version") != "2023-06-01" || request.Header.Get("Authorization") != "" {
					t.Errorf("target %s headers=%#v", target, request.Header)
				}
			} else if request.Header.Get("Authorization") != "Bearer "+secret || request.Header.Get("X-API-Key") != "" || request.Header.Get("Anthropic-Version") != "" {
				t.Errorf("target %s headers=%#v", target, request.Header)
			}
			return &http.Response{StatusCode: http.StatusOK, Header: make(http.Header), Body: io.NopCloser(strings.NewReader(`{"ok":true}`)), Request: request}, nil
		})}
		response, err := doMemberMessages(context.Background(), client, &PoolMember{
			UpstreamBaseURL: base, UpstreamKey: secret, UpstreamAuth: policy.Auth,
			UpstreamTransport: policy.Transport, UpstreamTarget: target, CredentialClass: policy.CredentialClass,
		}, policy.LocalEndpoint, []byte(`{"model":"synthetic"}`), false)
		if err != nil {
			t.Fatalf("target %s: %v", target, err)
		}
		_ = response.Body.Close()
	}
}

func TestCrossTargetMismatchFailsBeforeSendingAPIKey(t *testing.T) {
	var hits atomic.Int32
	client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
		hits.Add(1)
		return nil, nil
	})}
	_, err := doMemberMessages(context.Background(), client, &PoolMember{
		UpstreamBaseURL: "https://api.kimi.com/coding/v1", UpstreamKey: "must-not-send",
		UpstreamAuth: authBearer, UpstreamTransport: transportOpenAIChatCompletions,
		UpstreamTarget: upstreamTargetOpenAIAPI, CredentialClass: credentialClassAPIKey,
	}, "/v1/chat/completions", []byte(`{"model":"synthetic"}`), false)
	if err == nil || hits.Load() != 0 {
		t.Fatalf("err=%v hits=%d", err, hits.Load())
	}
}
