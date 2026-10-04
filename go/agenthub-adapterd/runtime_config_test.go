package main

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

const (
	configIngressMessages  = "ahb_config_messages_secret"
	configIngressResponses = "ahb_config_responses_secret"
	configIngressChat      = "ahb_config_chat_secret"
	configAliasMessages    = "ahb_config_messages_alias_secret"
	configAliasResponses   = "ahb_config_responses_alias_secret"
	configAliasChat        = "ahb_config_chat_alias_secret"
	configUpstreamMessages = "upstream_messages_secret"
	configUpstreamResponse = "upstream_responses_secret"
	configUpstreamChat     = "upstream_chat_secret"
)

type roundTripFunc func(*http.Request) (*http.Response, error)

func (fn roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return fn(request)
}

func configuredRuntime(t *testing.T, config *RuntimeConfig) (*Runtime, string, int64, int) {
	t.Helper()
	rt, epoch, term := ownedConfiguredRuntime(t, config)
	start := controlJSON(t, rt, map[string]any{
		"type":           typeStart,
		"request_id":     "config-start",
		"instance_epoch": epoch,
		"owner_id":       "config-owner",
		"owner_term":     term,
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"listen_port": 0},
	})
	if !start.OK {
		t.Fatalf("start: %+v", start.Error)
	}
	var payload struct {
		Port int `json:"port"`
	}
	if err := json.Unmarshal(start.Payload, &payload); err != nil {
		t.Fatal(err)
	}
	if payload.Port <= 0 || payload.Port == productDefaultPort {
		t.Fatalf("invalid configured port %d", payload.Port)
	}
	return rt, epoch, term, payload.Port
}

func ownedConfiguredRuntime(t *testing.T, config *RuntimeConfig) (*Runtime, string, int64) {
	t.Helper()
	rt := testRuntime(t)
	if err := rt.SetRuntimeConfig(config); err != nil {
		t.Fatal(err)
	}
	hs := handshakeOK(t, rt)
	acq := controlJSON(t, rt, map[string]any{
		"type":           typeAcquireOrRenewOwner,
		"request_id":     "config-acquire",
		"instance_epoch": hs.InstanceEpoch,
		"owner_id":       "config-owner",
		"app_data_dir":   rt.Home(),
		"payload":        map[string]any{"mode": "acquire", "lease_budget_ms": 60000},
	})
	if !acq.OK {
		t.Fatalf("acquire: %+v", acq.Error)
	}
	var acquired AcquireSuccess
	if err := json.Unmarshal(acq.Payload, &acquired); err != nil {
		t.Fatal(err)
	}
	return rt, hs.InstanceEpoch, acquired.OwnerTerm
}

func threeEdgeConfig(upstream string) *RuntimeConfig {
	return &RuntimeConfig{
		Version: runtimeConfigVersion,
		Edges: []RuntimeEdgeConfig{
			{
				ID: "messages-edge", IngressKey: configIngressMessages, IngressKeys: []string{configAliasMessages}, Surface: surfaceMessages,
				Dialect: "claude", SchedulePolicy: policyPriorityFailover, FixtureModel: "claude-config-model",
				Members: []RuntimeMemberConfig{{ID: "messages-member", UpstreamBaseURL: upstream, UpstreamKey: configUpstreamMessages, UpstreamAuth: authAPIKey, UpstreamTransport: transportAnthropicMessages, Models: []string{"claude-config-model"}}},
			},
			{
				ID: "responses-edge", IngressKey: configIngressResponses, IngressKeys: []string{configAliasResponses}, Surface: surfaceResponses,
				Dialect: "codex", SchedulePolicy: policyPriorityFailover, FixtureModel: "gpt-config-response",
				Members: []RuntimeMemberConfig{{ID: "responses-member", UpstreamBaseURL: upstream, UpstreamKey: configUpstreamResponse, UpstreamAuth: authBearer, UpstreamTransport: transportCodexResponses, Models: []string{"gpt-config-response"}}},
			},
			{
				ID: "chat-edge", IngressKey: configIngressChat, IngressKeys: []string{configAliasChat}, Surface: surfaceChatCompletions,
				Dialect: "generic", SchedulePolicy: policyRoundRobin, FixtureModel: "gpt-config-chat",
				Members: []RuntimeMemberConfig{{ID: "chat-member", UpstreamBaseURL: upstream, UpstreamKey: configUpstreamChat, UpstreamAuth: authBearer, UpstreamTransport: transportOpenAIChatCompletions, Models: []string{"gpt-config-chat"}}},
			},
		},
	}
}

func TestLoadRuntimeConfigValidatesSchemaWithoutEchoingSecrets(t *testing.T) {
	raw := `{"version":"route-config.v0-isolated","edges":[{"id":"edge","ingress_key":"ingress-secret","surface":"messages","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"model","members":[{"id":"member","upstream_base_url":"http://127.0.0.1:18080","upstream_key":"upstream-secret","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages","priority":0,"position":0,"models":["model"]}]}]}`
	config, err := LoadRuntimeConfig(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	if config.Edges[0].Members[0].UpstreamAuth != authAPIKey {
		t.Fatalf("auth=%q", config.Edges[0].Members[0].UpstreamAuth)
	}
	for _, invalid := range []string{
		strings.Replace(raw, runtimeConfigVersion, "bad-version", 1),
		raw + `{}`,
		strings.Replace(raw, `"surface":"messages"`, `"surface":"unknown"`, 1),
		strings.Replace(raw, `"upstream_auth":"x_api_key"`, `"upstream_auth":"oauth"`, 1),
		strings.Replace(raw, `"upstream_auth":"x_api_key"`, `"upstream_auth":"bearer"`, 1),
		strings.Replace(raw, `"upstream_transport":"anthropic_messages"`, `"upstream_transport":"codex_responses"`, 1),
		strings.Replace(raw, `http://127.0.0.1:18080`, `https://example.com`, 1),
	} {
		if _, err := LoadRuntimeConfig(strings.NewReader(invalid)); err == nil {
			t.Fatalf("expected invalid runtime config: %s", invalid)
		} else if strings.Contains(err.Error(), "ingress-secret") || strings.Contains(err.Error(), "upstream-secret") {
			t.Fatalf("validation error leaked a secret: %v", err)
		}
	}
}

func TestRuntimeConfigIngressAliasesAreDeduplicatedPerEdgeAndUniqueAcrossEdges(t *testing.T) {
	const primary = "ahb_alias_test_primary_synthetic"
	const alias = "ahb_alias_test_extra_synthetic"
	raw := `{"version":"route-config.v0-isolated","edges":[{"id":"edge","ingress_key":"` + primary + `","ingress_keys":["` + primary + `","` + alias + `","` + alias + `"],"surface":"messages","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"model","members":[{"id":"member","upstream_base_url":"http://127.0.0.1:18080","upstream_key":"upstream-secret","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages","priority":0,"position":0,"models":["model"]}]}]}`
	config, err := LoadRuntimeConfig(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	if got := config.Edges[0].IngressKeys; len(got) != 1 || got[0] != alias {
		t.Fatalf("deduplicated aliases=%#v", got)
	}

	otherEdge := func(ingressKey string, ingressKeys []string) RuntimeEdgeConfig {
		return RuntimeEdgeConfig{
			ID: "other", IngressKey: ingressKey, IngressKeys: ingressKeys,
			Surface: surfaceMessages, Dialect: "claude", FixtureModel: "model",
			Members: []RuntimeMemberConfig{{
				ID: "member-2", UpstreamBaseURL: "http://127.0.0.1:18080", UpstreamKey: "upstream-secret-2",
				UpstreamAuth: authAPIKey, UpstreamTransport: transportAnthropicMessages, Models: []string{"model"},
			}},
		}
	}
	for _, duplicate := range []RuntimeEdgeConfig{
		otherEdge(primary, nil),
		otherEdge("other-primary", []string{alias}),
	} {
		candidate, loadErr := LoadRuntimeConfig(strings.NewReader(raw))
		if loadErr != nil {
			t.Fatal(loadErr)
		}
		candidate.Edges = append(candidate.Edges, duplicate)
		if err := validateRuntimeConfig(candidate); err == nil {
			t.Fatal("accepted an ingress key assigned to multiple edges")
		} else if strings.Contains(err.Error(), primary) || strings.Contains(err.Error(), alias) {
			t.Fatalf("cross-edge conflict leaked an ingress key: %v", err)
		}
	}

	emptyAlias := strings.Replace(raw, `"ingress_keys":["`+primary+`","`+alias+`","`+alias+`"]`, `"ingress_keys":[""]`, 1)
	if _, err := LoadRuntimeConfig(strings.NewReader(emptyAlias)); err == nil {
		t.Fatal("accepted an empty ingress alias")
	}
}

func TestRuntimeConfigRejectionMessageDoesNotEchoInput(t *testing.T) {
	raw := `{"version":"route-config.v0-isolated","edges":[{"id":"malicious-id-must-not-echo","ingress_key":"secret","surface":"messages","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"model","members":[]}],"unknown-field-must-not-echo":true}`
	config, digest, rejection := loadRuntimeConfigForRun(strings.NewReader(raw))
	if config != nil || rejection != runtimeConfigRejectedMessage {
		t.Fatalf("config=%v digest=%q rejection=%q", config, digest, rejection)
	}
	for _, attackerControlled := range []string{"malicious-id-must-not-echo", "unknown-field-must-not-echo"} {
		if strings.Contains(rejection, attackerControlled) {
			t.Fatalf("rejection echoed attacker-controlled input: %q", rejection)
		}
	}
}

func TestOneShotRuntimeConfigHasCompleteStatusIdentity(t *testing.T) {
	raw := streamRuntimeConfigJSON("edge-one-shot", streamIngressSecret, streamUpstreamSecret)
	config, digest, rejection := loadRuntimeConfigForRun(bytes.NewReader(raw))
	if rejection != "" || config == nil {
		t.Fatalf("config=%v rejection=%q", config, rejection)
	}
	if digest != runtimeConfigDigest(raw) {
		t.Fatalf("digest=%q", digest)
	}
	rt, err := NewRuntime(t.TempDir(), 0, "", func() {})
	if err != nil {
		t.Fatal(err)
	}
	if err := rt.SetRuntimeConfigWithDigest(config, digest); err != nil {
		t.Fatal(err)
	}
	snapshot, err := rt.statusSnapshot()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.ActiveRevision == nil || *snapshot.ActiveRevision != "1" || snapshot.ActiveHash == nil || *snapshot.ActiveHash != digest {
		t.Fatalf("one-shot status identity is incomplete: %+v", snapshot)
	}
}

func TestRuntimeConfigDialectMustMatchSurface(t *testing.T) {
	raw := `{"version":"route-config.v0-isolated","edges":[{"id":"edge","ingress_key":"secret","surface":"messages","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"model","members":[{"id":"member","upstream_base_url":"http://127.0.0.1:18080","upstream_key":"upstream","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages","priority":0,"position":0,"models":["model"]}]}]}`
	for _, mismatch := range []string{`"dialect":"codex"`, `"dialect":"generic"`, `"dialect":"anthropic"`} {
		invalid := strings.Replace(raw, `"dialect":"claude"`, mismatch, 1)
		if _, err := LoadRuntimeConfig(strings.NewReader(invalid)); err == nil {
			t.Fatalf("accepted messages config with %s", mismatch)
		}
	}
}

func TestRuntimeConfigAllowsResponsesToChatTransport(t *testing.T) {
	raw := `{"version":"route-config.v0-isolated","edges":[{"id":"edge","ingress_key":"secret","surface":"responses","dialect":"codex","schedule_policy":"priority_failover","fixture_model":"model","members":[{"id":"member","upstream_base_url":"http://127.0.0.1:18080","upstream_key":"upstream","upstream_auth":"bearer","upstream_transport":"openai_chat_completions","priority":0,"position":0,"models":["model"]}]}]}`
	config, err := LoadRuntimeConfig(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	if got := config.Edges[0].Members[0].UpstreamTransport; got != transportOpenAIChatCompletions {
		t.Fatalf("transport=%q", got)
	}

	invalid := strings.Replace(raw, `"surface":"responses"`, `"surface":"messages"`, 1)
	invalid = strings.Replace(invalid, `"dialect":"codex"`, `"dialect":"claude"`, 1)
	invalid = strings.Replace(invalid, `"upstream_auth":"bearer"`, `"upstream_auth":"x_api_key"`, 1)
	if _, err := LoadRuntimeConfig(strings.NewReader(invalid)); err == nil {
		t.Fatal("accepted Messages surface with Chat Completions transport")
	}
}

func TestRuntimeUpstreamURLPolicy(t *testing.T) {
	for _, allowed := range []struct {
		url, transport string
	}{
		{"http://127.0.0.1:18080/v1", transportCodexResponses},
		{"https://api.anthropic.com", transportAnthropicMessages},
		{"https://API.ANTHROPIC.COM/", transportAnthropicMessages},
		{"https://api.anthropic.com:443/v1", transportAnthropicMessages},
	} {
		if err := validateRuntimeUpstreamURL(allowed.url, allowed.transport); err != nil {
			t.Errorf("allowed URL %q rejected: %v", allowed.url, err)
		}
	}
	for _, denied := range []struct {
		url, transport string
	}{
		{"http://api.anthropic.com/v1", transportAnthropicMessages},
		{"http://user@127.0.0.1:18080/custom", transportCodexResponses},
		{"http://127.0.0.1:18080/custom?key=value", transportCodexResponses},
		{"http://127.0.0.1:18080/custom?", transportCodexResponses},
		{"http://127.0.0.1:18080/custom#fragment", transportCodexResponses},
		{"http://127.0.0.1:18080/custom#", transportCodexResponses},
		{"https://api.anthropic.com.evil.example/v1", transportAnthropicMessages},
		{"https://user@api.anthropic.com/v1", transportAnthropicMessages},
		{"https://@api.anthropic.com/v1", transportAnthropicMessages},
		{"https://api.anthropic.com/v1?key=value", transportAnthropicMessages},
		{"https://api.anthropic.com/v1#fragment", transportAnthropicMessages},
		{"https://api.anthropic.com:8443/v1", transportAnthropicMessages},
		{"https://api.anthropic.com:/v1", transportAnthropicMessages},
		{"https://api.anthropic.com/v1/", transportAnthropicMessages},
		{"https://api.anthropic.com/%76%31", transportAnthropicMessages},
		{"https://api.anthropic.com/v1", transportCodexResponses},
		{"https://chatgpt.com/backend-api/codex", transportCodexResponses},
		{"https://cli-chat-proxy.grok.com/v1", transportGrokResponses},
	} {
		if err := validateRuntimeUpstreamURL(denied.url, denied.transport); err == nil {
			t.Errorf("denied URL %q transport %q was accepted", denied.url, denied.transport)
		}
	}
}

func TestOfficialAnthropicConfigAndHeaders(t *testing.T) {
	raw := `{"version":"route-config.v0-isolated","edges":[{"id":"edge","ingress_key":"ingress","surface":"messages","dialect":"claude","schedule_policy":"priority_failover","fixture_model":"claude-model","members":[{"id":"member","upstream_base_url":"https://api.anthropic.com/v1","upstream_key":"synthetic-anthropic-key","upstream_auth":"x_api_key","upstream_transport":"anthropic_messages","priority":0,"position":0,"models":["claude-model"]}]}]}`
	config, err := LoadRuntimeConfig(strings.NewReader(raw))
	if err != nil {
		t.Fatal(err)
	}
	member := config.Edges[0].Members[0]
	client := &http.Client{Transport: roundTripFunc(func(request *http.Request) (*http.Response, error) {
		if request.URL.String() != "https://api.anthropic.com/v1/messages" {
			t.Errorf("upstream URL=%q", request.URL.String())
		}
		if request.Header.Get("X-API-Key") != member.UpstreamKey || request.Header.Get("Anthropic-Version") != "2023-06-01" {
			t.Errorf("Anthropic headers=%#v", request.Header)
		}
		if request.Header.Get("Authorization") != "" {
			t.Errorf("unexpected Authorization header")
		}
		return &http.Response{
			StatusCode: http.StatusOK,
			Header:     make(http.Header),
			Body:       io.NopCloser(strings.NewReader(`{"ok":true}`)),
			Request:    request,
		}, nil
	})}
	response, err := doMemberMessages(context.Background(), client, &PoolMember{
		UpstreamBaseURL:   member.UpstreamBaseURL,
		UpstreamKey:       member.UpstreamKey,
		UpstreamAuth:      member.UpstreamAuth,
		UpstreamTransport: member.UpstreamTransport,
	}, "/v1/messages", []byte(`{"model":"claude-model"}`), false)
	if err != nil {
		t.Fatal(err)
	}
	_ = response.Body.Close()
}

func TestLegacyProbeAPIKeyStillAddsAnthropicVersion(t *testing.T) {
	pool, err := NewPoolFromFixture(ProbeFixture{
		FixtureModel: "legacy-model",
		Members: []ProbeMember{{
			ID:              "legacy-member",
			UpstreamBaseURL: "http://127.0.0.1:18080/v1",
			UpstreamKey:     "legacy-synthetic-key",
			UpstreamAuth:    authAPIKey,
			Models:          []string{"legacy-model"},
		}},
	})
	if err != nil {
		t.Fatal(err)
	}
	member := pool.Pick("legacy-model", nil, time.Now())
	if member == nil || member.UpstreamTransport != transportAnthropicMessages {
		t.Fatalf("legacy member transport was not restored: %+v", member)
	}
	client := &http.Client{Transport: roundTripFunc(func(request *http.Request) (*http.Response, error) {
		if request.Header.Get("X-API-Key") != "legacy-synthetic-key" || request.Header.Get("Anthropic-Version") != "2023-06-01" {
			t.Errorf("legacy Anthropic headers=%#v", request.Header)
		}
		if request.Header.Get("Authorization") != "" {
			t.Errorf("legacy request unexpectedly sent Authorization")
		}
		return &http.Response{
			StatusCode: http.StatusOK,
			Header:     make(http.Header),
			Body:       io.NopCloser(strings.NewReader(`{"ok":true}`)),
			Request:    request,
		}, nil
	})}
	response, err := doMemberMessages(context.Background(), client, member, "/v1/messages", []byte(`{"model":"legacy-model"}`), false)
	if err != nil {
		t.Fatal(err)
	}
	_ = response.Body.Close()
}

func TestUpstreamClientDoesNotFollowRedirects(t *testing.T) {
	targetHit := make(chan struct{}, 1)
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		targetHit <- struct{}{}
		w.WriteHeader(http.StatusOK)
	}))
	t.Cleanup(target.Close)
	redirect := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Redirect(w, &http.Request{}, target.URL, http.StatusTemporaryRedirect)
	}))
	t.Cleanup(redirect.Close)

	response, err := doMemberMessages(context.Background(), newUpstreamHTTPClient(), &PoolMember{
		UpstreamBaseURL:   redirect.URL,
		UpstreamKey:       "synthetic-key",
		UpstreamAuth:      authAPIKey,
		UpstreamTransport: transportAnthropicMessages,
	}, "/v1/messages", []byte(`{"model":"claude-model"}`), false)
	if err != nil {
		t.Fatal(err)
	}
	_ = response.Body.Close()
	if response.StatusCode != http.StatusTemporaryRedirect {
		t.Fatalf("redirect status=%d", response.StatusCode)
	}
	select {
	case <-targetHit:
		t.Fatal("upstream client followed redirect and risked resending the API key")
	default:
	}
}

func TestJoinUpstreamPathAvoidsDuplicateV1(t *testing.T) {
	for _, item := range []struct {
		base, endpoint, want string
	}{
		{"http://127.0.0.1:18080", "/v1/messages", "http://127.0.0.1:18080/v1/messages"},
		{"http://127.0.0.1:18080/", "/v1/responses", "http://127.0.0.1:18080/v1/responses"},
		{"http://127.0.0.1:18080/v1", "/v1/messages", "http://127.0.0.1:18080/v1/messages"},
		{"http://127.0.0.1:18080/v1/", "/v1/chat/completions", "http://127.0.0.1:18080/v1/chat/completions"},
	} {
		if got := joinUpstreamPath(item.base, item.endpoint); got != item.want {
			t.Fatalf("joinUpstreamPath(%q, %q)=%q want %q", item.base, item.endpoint, got, item.want)
		}
	}
}

func TestRuntimeConfigRoutesThreeSurfacesAndAuthModes(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		var request struct {
			Model string `json:"model"`
		}
		_ = json.Unmarshal(body, &request)
		switch r.URL.Path {
		case "/v1/messages":
			if r.Header.Get("X-API-Key") != configUpstreamMessages || r.Header.Get("Anthropic-Version") != "2023-06-01" || r.Header.Get("Authorization") != "" {
				t.Errorf("messages auth headers: %#v", r.Header)
			}
		case "/v1/responses":
			if bearerToken(r.Header.Get("Authorization")) != configUpstreamResponse || r.Header.Get("X-API-Key") != "" {
				t.Errorf("responses auth headers: %#v", r.Header)
			}
		case "/v1/chat/completions":
			if bearerToken(r.Header.Get("Authorization")) != configUpstreamChat || r.Header.Get("X-API-Key") != "" {
				t.Errorf("chat auth headers: %#v", r.Header)
			}
		default:
			t.Errorf("unexpected upstream path %s", r.URL.Path)
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"ok": true, "model": request.Model, "path": r.URL.Path})
	}))
	t.Cleanup(upstream.Close)
	rt, epoch, term, port := configuredRuntime(t, threeEdgeConfig(upstream.URL))

	requests := []struct {
		path, key, model string
	}{
		{"/v1/messages", configIngressMessages, "claude-config-model"},
		{"/v1/messages", configAliasMessages, "claude-config-model"},
		{"/v1/responses", configIngressResponses, "gpt-config-response"},
		{"/v1/responses", configAliasResponses, "gpt-config-response"},
		{"/v1/chat/completions", configIngressChat, "gpt-config-chat"},
		{"/v1/chat/completions", configAliasChat, "gpt-config-chat"},
	}
	for _, item := range requests {
		body := bytes.NewBufferString(`{"model":"` + item.model + `","stream":false}`)
		req, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+item.path, body)
		req.Header.Set("Authorization", "Bearer "+item.key)
		req.Header.Set("Content-Type", "application/json")
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		got, _ := io.ReadAll(resp.Body)
		_ = resp.Body.Close()
		if resp.StatusCode != http.StatusOK || !bytes.Contains(got, []byte(item.model)) {
			t.Fatalf("%s status=%d body=%s", item.path, resp.StatusCode, got)
		}

		modelsReq, _ := http.NewRequest(http.MethodGet, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/models", nil)
		modelsReq.Header.Set("Authorization", "Bearer "+item.key)
		modelsResp, err := http.DefaultClient.Do(modelsReq)
		if err != nil {
			t.Fatal(err)
		}
		modelsBody, _ := io.ReadAll(modelsResp.Body)
		_ = modelsResp.Body.Close()
		if modelsResp.StatusCode != http.StatusOK || !bytes.Contains(modelsBody, []byte(item.model)) {
			t.Fatalf("models %s status=%d body=%s", item.model, modelsResp.StatusCode, modelsBody)
		}
		for _, other := range requests {
			if other.model != item.model && bytes.Contains(modelsBody, []byte(other.model)) {
				t.Fatalf("models for %s leaked edge model %s: %s", item.model, other.model, modelsBody)
			}
		}
	}

	wrong, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/responses", strings.NewReader(`{"model":"gpt-config-response"}`))
	wrong.Header.Set("Authorization", "Bearer "+configIngressMessages)
	wrongResp, err := http.DefaultClient.Do(wrong)
	if err != nil {
		t.Fatal(err)
	}
	_ = wrongResp.Body.Close()
	if wrongResp.StatusCode != http.StatusUnauthorized {
		t.Fatalf("cross-surface bearer status=%d", wrongResp.StatusCode)
	}

	status := controlJSON(t, rt, map[string]any{
		"type": typeStatus, "request_id": "config-status", "instance_epoch": epoch,
		"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{},
	})
	if !status.OK {
		t.Fatalf("status: %+v", status.Error)
	}
	for _, secret := range []string{
		configIngressMessages, configIngressResponses, configIngressChat,
		configAliasMessages, configAliasResponses, configAliasChat,
		configUpstreamMessages, configUpstreamResponse, configUpstreamChat,
	} {
		if bytes.Contains(status.Payload, []byte(secret)) {
			t.Fatalf("status leaked secret: %s", status.Payload)
		}
	}
	var snapshot StatusSuccess
	if err := json.Unmarshal(status.Payload, &snapshot); err != nil {
		t.Fatal(err)
	}
	if snapshot.MemberCount != 3 || snapshot.HealthyMemberCount != 3 {
		t.Fatalf("status counts: %+v", snapshot)
	}
}

func TestRuntimeConfigConvertsResponsesToChatWithoutLeakingBadUpstreamBody(t *testing.T) {
	const upstreamSecret = "upstream-body-secret-must-not-escape"
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/chat/completions" {
			t.Errorf("upstream path=%s", r.URL.Path)
		}
		if bearerToken(r.Header.Get("Authorization")) != configUpstreamChat {
			t.Errorf("upstream auth headers: %#v", r.Header)
		}
		var request struct {
			Model    string `json:"model"`
			Messages []struct {
				Role    string `json:"role"`
				Content string `json:"content"`
			} `json:"messages"`
		}
		if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
			t.Errorf("decode Chat request: %v", err)
		}
		if len(request.Messages) != 1 || request.Messages[0].Role != "user" {
			t.Errorf("translated messages=%#v", request.Messages)
		}
		if request.Messages[0].Content == "malformed" {
			w.Header().Set("Content-Type", "application/json")
			_, _ = io.WriteString(w, `{"choices":"`+upstreamSecret+`"}`)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"id": "chatcmpl_conversion", "model": request.Model, "created": 7,
			"choices": []any{map[string]any{
				"message":       map[string]any{"role": "assistant", "content": "converted-ok"},
				"finish_reason": "stop",
			}},
		})
	}))
	t.Cleanup(upstream.Close)
	config := &RuntimeConfig{Version: runtimeConfigVersion, Edges: []RuntimeEdgeConfig{{
		ID: "responses-chat-edge", IngressKey: configIngressResponses, Surface: surfaceResponses,
		Dialect: "codex", SchedulePolicy: policyPriorityFailover, FixtureModel: "gpt-converted",
		Members: []RuntimeMemberConfig{{
			ID: "chat-upstream", UpstreamBaseURL: upstream.URL, UpstreamKey: configUpstreamChat,
			UpstreamAuth: authBearer, UpstreamTransport: transportOpenAIChatCompletions,
			Models: []string{"gpt-converted"},
		}},
	}}}
	_, _, _, port := configuredRuntime(t, config)

	post := func(input string) (int, []byte) {
		t.Helper()
		body, _ := json.Marshal(map[string]any{"model": "gpt-converted", "input": input})
		req, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/responses", bytes.NewReader(body))
		req.Header.Set("Authorization", "Bearer "+configIngressResponses)
		req.Header.Set("Content-Type", "application/json")
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		defer resp.Body.Close()
		got, _ := io.ReadAll(resp.Body)
		return resp.StatusCode, got
	}

	status, body := post("ping")
	if status != http.StatusOK || !bytes.Contains(body, []byte(`"object":"response"`)) || !bytes.Contains(body, []byte("converted-ok")) {
		t.Fatalf("converted status=%d body=%s", status, body)
	}
	status, body = post("malformed")
	if status != http.StatusBadGateway || bytes.Contains(body, []byte(upstreamSecret)) || !bytes.Contains(body, []byte(`"code":"upstream_error"`)) {
		t.Fatalf("safe upstream error status=%d body=%s", status, body)
	}
}

func TestRuntimeConfigConvertedStreamSignalsTruncationAfterCommit(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/event-stream")
		w.WriteHeader(http.StatusOK)
		_, _ = io.WriteString(w, `data: {"id":"chatcmpl_truncated","model":"gpt-converted","choices":[{"delta":{"content":"partial"}}]}`+"\n\n")
		if flusher, ok := w.(http.Flusher); ok {
			flusher.Flush()
		}
		// Deliberately close without Chat's [DONE] marker.
	}))
	t.Cleanup(upstream.Close)
	config := &RuntimeConfig{Version: runtimeConfigVersion, Edges: []RuntimeEdgeConfig{{
		ID: "responses-chat-edge", IngressKey: configIngressResponses, Surface: surfaceResponses,
		Dialect: "codex", SchedulePolicy: policyPriorityFailover, FixtureModel: "gpt-converted",
		Members: []RuntimeMemberConfig{{
			ID: "chat-upstream", UpstreamBaseURL: upstream.URL, UpstreamKey: configUpstreamChat,
			UpstreamAuth: authBearer, UpstreamTransport: transportOpenAIChatCompletions,
			Models: []string{"gpt-converted"},
		}},
	}}}
	_, _, _, port := configuredRuntime(t, config)
	body := []byte(`{"model":"gpt-converted","input":"ping","stream":true}`)
	req, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/responses", bytes.NewReader(body))
	req.Header.Set("Authorization", "Bearer "+configIngressResponses)
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	got, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK || !bytes.Contains(got, []byte("response.output_text.delta")) {
		t.Fatalf("translated stream status=%d body=%s", resp.StatusCode, got)
	}
	if !bytes.Contains(got, []byte("event: error")) || !bytes.Contains(got, []byte(`"code":"upstream_error"`)) {
		t.Fatalf("truncation was silent: %s", got)
	}
	if bytes.Contains(got, []byte("response.completed")) {
		t.Fatalf("truncated stream was completed: %s", got)
	}
	healthReq, _ := http.NewRequest(http.MethodGet, "http://127.0.0.1:"+strconv.Itoa(port)+"/health", nil)
	healthReq.Header.Set("Authorization", "Bearer "+configIngressResponses)
	healthResp, err := http.DefaultClient.Do(healthReq)
	if err != nil {
		t.Fatal(err)
	}
	defer healthResp.Body.Close()
	var health map[string]any
	if err := json.NewDecoder(healthResp.Body).Decode(&health); err != nil {
		t.Fatal(err)
	}
	if health["healthy_member_count"] != float64(0) {
		t.Fatalf("truncated upstream remained healthy: %#v", health)
	}
}

func TestRuntimeConfigSwapIsAtomicForInFlightRequests(t *testing.T) {
	oldEntered := make(chan struct{})
	releaseOld := make(chan struct{})
	oldUpstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		select {
		case <-oldEntered:
		default:
			close(oldEntered)
		}
		<-releaseOld
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"source":"old"}`)
	}))
	t.Cleanup(oldUpstream.Close)
	newUpstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"source":"new"}`)
	}))
	t.Cleanup(newUpstream.Close)

	configFor := func(id, upstream, secret string) *RuntimeConfig {
		return &RuntimeConfig{Version: runtimeConfigVersion, Edges: []RuntimeEdgeConfig{{
			ID: "atomic-edge", IngressKey: configIngressResponses, Surface: surfaceResponses,
			Dialect: "codex", SchedulePolicy: policyPriorityFailover, FixtureModel: "gpt-atomic",
			Members: []RuntimeMemberConfig{{
				ID: id, UpstreamBaseURL: upstream, UpstreamKey: secret,
				UpstreamAuth: authBearer, UpstreamTransport: transportCodexResponses,
				Models: []string{"gpt-atomic"},
			}},
		}}}
	}
	rt, _, _, port := configuredRuntime(t, configFor("old-member", oldUpstream.URL, configUpstreamResponse))
	post := func() (int, []byte, error) {
		body := bytes.NewBufferString(`{"model":"gpt-atomic","input":"ping"}`)
		req, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/responses", body)
		req.Header.Set("Authorization", "Bearer "+configIngressResponses)
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			return 0, nil, err
		}
		defer resp.Body.Close()
		got, err := io.ReadAll(resp.Body)
		return resp.StatusCode, got, err
	}

	oldResult := make(chan []byte, 1)
	oldError := make(chan error, 1)
	go func() {
		status, body, err := post()
		if err != nil {
			oldError <- err
			return
		}
		if status != http.StatusOK {
			oldError <- fmt.Errorf("old request status=%d body=%s", status, body)
			return
		}
		oldResult <- body
	}()
	select {
	case <-oldEntered:
	case <-time.After(2 * time.Second):
		t.Fatal("old request did not reach upstream")
	}

	bad := configFor("bad-member", newUpstream.URL, configUpstreamChat)
	bad.Edges[0].Surface = surfaceMessages
	if err := rt.SwapRuntimeConfig(bad, "bad-digest"); err == nil {
		t.Fatal("invalid hot config was accepted")
	}
	if err := rt.SwapRuntimeConfig(configFor("new-member", newUpstream.URL, configUpstreamChat), "new-digest"); err != nil {
		t.Fatal(err)
	}
	status, body, err := post()
	if err != nil || status != http.StatusOK || !bytes.Contains(body, []byte(`"source":"new"`)) {
		t.Fatalf("new request status=%d body=%s err=%v", status, body, err)
	}
	close(releaseOld)
	select {
	case err := <-oldError:
		t.Fatal(err)
	case body := <-oldResult:
		if !bytes.Contains(body, []byte(`"source":"old"`)) {
			t.Fatalf("in-flight request changed edge: %s", body)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("old request did not complete")
	}

	statusReply := controlJSON(t, rt, map[string]any{
		"type": typeStatus, "request_id": "atomic-status", "instance_epoch": rt.instanceEpoch,
		"owner_id": "config-owner", "owner_term": int64(1), "app_data_dir": rt.Home(), "payload": map[string]any{},
	})
	if !statusReply.OK {
		t.Fatalf("status: %+v", statusReply.Error)
	}
	var snapshot StatusSuccess
	if err := json.Unmarshal(statusReply.Payload, &snapshot); err != nil {
		t.Fatal(err)
	}
	if snapshot.Port == nil || *snapshot.Port != port || snapshot.ActiveHash == nil || *snapshot.ActiveHash != "new-digest" {
		t.Fatalf("status after atomic swap: %+v", snapshot)
	}
	if bytes.Contains(statusReply.Payload, []byte(configUpstreamChat)) || bytes.Contains(statusReply.Payload, []byte(configIngressResponses)) {
		t.Fatalf("status leaked secret: %s", statusReply.Payload)
	}
}

func TestRuntimeConfigSwapWaitsForLifecycleTransition(t *testing.T) {
	rt, err := NewRuntime(t.TempDir(), 0, "", func() {})
	if err != nil {
		t.Fatal(err)
	}
	config, err := LoadRuntimeConfig(bytes.NewReader(streamRuntimeConfigJSON(
		"serialized-edge", streamIngressSecret, streamUpstreamSecret,
	)))
	if err != nil {
		t.Fatal(err)
	}

	rt.lifecycleMu.Lock()
	started := make(chan struct{})
	done := make(chan error, 1)
	go func() {
		close(started)
		done <- rt.SwapRuntimeConfig(config, "serialized-digest")
	}()
	<-started
	select {
	case err := <-done:
		rt.lifecycleMu.Unlock()
		t.Fatalf("swap crossed an active lifecycle transition: %v", err)
	case <-time.After(50 * time.Millisecond):
	}
	rt.lifecycleMu.Unlock()
	if err := <-done; err != nil {
		t.Fatal(err)
	}
	rt.mu.Lock()
	defer rt.mu.Unlock()
	if rt.configHash != "serialized-digest" || len(rt.edges) != 1 || rt.edges[0].ID != "serialized-edge" {
		t.Fatalf("serialized swap did not commit: hash=%q edges=%+v", rt.configHash, rt.edges)
	}
}

func TestConcurrentStartUsesOneListenerAndStopPreventsRestart(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	t.Cleanup(upstream.Close)
	config := threeEdgeConfig(upstream.URL)
	config.Edges = config.Edges[:1]
	rt, epoch, term := ownedConfiguredRuntime(t, config)

	const starts = 16
	replies := make(chan Reply, starts)
	var wg sync.WaitGroup
	for i := 0; i < starts; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			replies <- controlJSON(t, rt, map[string]any{
				"type": typeStart, "request_id": fmt.Sprintf("concurrent-start-%d", i), "instance_epoch": epoch,
				"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{"listen_port": 0},
			})
		}(i)
	}
	wg.Wait()
	close(replies)
	port := 0
	for reply := range replies {
		if !reply.OK {
			t.Fatalf("concurrent start: %+v", reply.Error)
		}
		var payload struct {
			Port int `json:"port"`
		}
		if err := json.Unmarshal(reply.Payload, &payload); err != nil {
			t.Fatal(err)
		}
		if port == 0 {
			port = payload.Port
		} else if payload.Port != port {
			t.Fatalf("concurrent starts created multiple listeners: %d and %d", port, payload.Port)
		}
	}
	stop := controlJSON(t, rt, map[string]any{
		"type": typeStop, "request_id": "concurrent-stop", "instance_epoch": epoch,
		"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{},
	})
	if !stop.OK {
		t.Fatalf("stop: %+v", stop.Error)
	}
	restart := controlJSON(t, rt, map[string]any{
		"type": typeStart, "request_id": "restart-during-drain", "instance_epoch": epoch,
		"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{"listen_port": 0},
	})
	if restart.OK || restart.Error == nil || restart.Error.Code != errLifecycleConflict {
		t.Fatalf("start during drain was not rejected: %+v", restart)
	}
}

func TestConcurrentStartAndStopAreSerialized(t *testing.T) {
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	t.Cleanup(upstream.Close)
	config := threeEdgeConfig(upstream.URL)
	config.Edges = config.Edges[:1]
	rt, epoch, term := ownedConfiguredRuntime(t, config)
	startRaw, err := json.Marshal(map[string]any{
		"type": typeStart, "request_id": "racing-start", "instance_epoch": epoch,
		"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{"listen_port": 0},
	})
	if err != nil {
		t.Fatal(err)
	}
	stopRaw, err := json.Marshal(map[string]any{
		"type": typeStop, "request_id": "racing-stop", "instance_epoch": epoch,
		"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{},
	})
	if err != nil {
		t.Fatal(err)
	}

	gate := make(chan struct{})
	startReply := make(chan Reply, 1)
	stopReply := make(chan Reply, 1)
	go func() {
		<-gate
		startReply <- rt.HandleControl(startRaw)
	}()
	go func() {
		<-gate
		stopReply <- rt.HandleControl(stopRaw)
	}()
	close(gate)
	start := <-startReply
	stop := <-stopReply
	if !stop.OK {
		t.Fatalf("racing stop failed: %+v", stop.Error)
	}
	if !start.OK && (start.Error == nil || start.Error.Code != errLifecycleConflict) {
		t.Fatalf("racing start failed unexpectedly: %+v", start.Error)
	}
	snapshot, err := rt.statusSnapshot()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.ListenReady || (snapshot.Lifecycle != lifecycleDraining && snapshot.Lifecycle != lifecycleStopped) {
		t.Fatalf("start/stop were not serialized: %+v", snapshot)
	}
}

func TestStopRejectsNewRequestsAndDrainsInflight(t *testing.T) {
	entered := make(chan struct{}, 1)
	release := make(chan struct{})
	upstream := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		entered <- struct{}{}
		select {
		case <-release:
		case <-r.Context().Done():
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"drained":true}`))
	}))
	t.Cleanup(upstream.Close)
	config := threeEdgeConfig(upstream.URL)
	config.Edges = config.Edges[:1]
	rt, epoch, term, port := configuredRuntime(t, config)

	requestDone := make(chan error, 1)
	go func() {
		req, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", strings.NewReader(`{"model":"claude-config-model"}`))
		req.Header.Set("Authorization", "Bearer "+configIngressMessages)
		resp, err := http.DefaultClient.Do(req)
		if err == nil {
			_, _ = io.Copy(io.Discard, resp.Body)
			_ = resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				err = errString("inflight request did not complete")
			}
		}
		requestDone <- err
	}()
	select {
	case <-entered:
	case <-time.After(2 * time.Second):
		t.Fatal("request did not enter upstream")
	}
	if snap, _ := rt.statusSnapshot(); snap.InFlightCount != 1 {
		t.Fatalf("in_flight before stop=%d", snap.InFlightCount)
	}
	stop := controlJSON(t, rt, map[string]any{
		"type": typeStop, "request_id": "config-stop", "instance_epoch": epoch,
		"owner_id": "config-owner", "owner_term": term, "app_data_dir": rt.Home(), "payload": map[string]any{},
	})
	if !stop.OK {
		t.Fatalf("stop: %+v", stop.Error)
	}
	if snap, _ := rt.statusSnapshot(); snap.Lifecycle != lifecycleDraining || snap.InFlightCount != 1 || snap.ListenReady {
		t.Fatalf("draining snapshot: %+v", snap)
	}
	newReq, _ := http.NewRequest(http.MethodPost, "http://127.0.0.1:"+strconv.Itoa(port)+"/v1/messages", strings.NewReader(`{"model":"claude-config-model"}`))
	newReq.Header.Set("Authorization", "Bearer "+configIngressMessages)
	newResp, newErr := (&http.Client{Timeout: time.Second}).Do(newReq)
	if newErr == nil {
		_ = newResp.Body.Close()
		if newResp.StatusCode != http.StatusServiceUnavailable {
			t.Fatalf("new request during drain status=%d", newResp.StatusCode)
		}
	}
	close(release)
	select {
	case err := <-requestDone:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("inflight request was not drained")
	}
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		snap, _ := rt.statusSnapshot()
		if snap.Lifecycle == lifecycleStopped && snap.InFlightCount == 0 {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatal("runtime did not finish draining")
}

func TestUnexpectedListenerFailureBecomesVisible(t *testing.T) {
	rt := testRuntime(t)
	cancelled := make(chan struct{})
	rt.mu.Lock()
	rt.lifecycle = lifecycleServing
	rt.listenReady = true
	rt.cancel = func() { close(cancelled) }
	rt.mu.Unlock()

	rt.recordServeFailure()
	select {
	case <-cancelled:
	case <-time.After(time.Second):
		t.Fatal("listener failure did not stop the runner")
	}
	snapshot, err := rt.statusSnapshot()
	if err != nil {
		t.Fatal(err)
	}
	if snapshot.ListenReady || snapshot.Lifecycle != lifecycleNotServing {
		t.Fatalf("listener failure snapshot: %+v", snapshot)
	}
	if snapshot.LastError == nil || snapshot.LastError.Code != errListenerFailed {
		t.Fatalf("listener failure missing stable error: %+v", snapshot.LastError)
	}
}
