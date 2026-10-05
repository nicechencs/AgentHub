package main

import (
	"fmt"
	"net"
	"net/url"
	"os"
	"path/filepath"
	"strings"
)

func resolveAbsolute(path string) (string, error) {
	if strings.TrimSpace(path) == "" {
		return "", fmt.Errorf("path is empty")
	}
	if !filepath.IsAbs(path) {
		return "", fmt.Errorf("path must be absolute: %s", path)
	}
	cleaned := filepath.Clean(path)
	if resolved, err := filepath.EvalSymlinks(cleaned); err == nil {
		return resolved, nil
	}
	return cleaned, nil
}

func realUserAgentHub() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return resolveAbsolute(filepath.Join(home, ".agenthub"))
}

func isForbiddenUserHome(path string) bool {
	resolved, err := resolveAbsolute(path)
	if err != nil {
		return true
	}
	realHome, err := realUserAgentHub()
	if err != nil {
		return false
	}
	rel, err := filepath.Rel(realHome, resolved)
	if err != nil {
		return false
	}
	return rel == "." || (rel != ".." && !strings.HasPrefix(rel, ".."+string(os.PathSeparator)))
}

func isUnderRoot(path, root string) bool {
	resolved, err := resolveAbsolute(path)
	if err != nil {
		return false
	}
	rootResolved, err := resolveAbsolute(root)
	if err != nil {
		rootResolved = filepath.Clean(root)
	}
	rel, err := filepath.Rel(rootResolved, resolved)
	if err != nil {
		return false
	}
	return rel == "." || (rel != ".." && !strings.HasPrefix(rel, ".."+string(os.PathSeparator)))
}

func isScratchHome(path string) bool {
	if isForbiddenUserHome(path) {
		return false
	}
	resolved, err := resolveAbsolute(path)
	if err != nil {
		return false
	}
	for _, root := range scratchRoots() {
		if isUnderRoot(resolved, root) {
			return true
		}
	}
	return isRepoProbeScratch(resolved)
}

func isProductRuntimeHome(path string) bool {
	resolved, err := resolveAbsolute(path)
	if err != nil || filepath.Base(resolved) != "adapterd" {
		return false
	}
	return filepath.Base(filepath.Dir(resolved)) == "runtime"
}

func scratchRoots() []string {
	roots := []string{"/tmp", "/var/tmp"}
	if tmp := os.TempDir(); tmp != "" {
		roots = append(roots, tmp)
	}
	return roots
}

func isRepoProbeScratch(path string) bool {
	slash := filepath.ToSlash(filepath.Clean(path))
	return strings.Contains(slash, "/.tmp/route-runtime-probe/")
}

func defaultControlSocket(home string) string {
	return filepath.Join(home, "run", "adapterd.sock")
}

func defaultPIDFile(home string) string {
	return filepath.Join(home, "run", "adapterd.pid")
}

func defaultLogFile(home string) string {
	return filepath.Join(home, "logs", "adapterd.log")
}

func defaultProbeFixture(home string) string {
	return filepath.Join(home, "config", "probe.json")
}

func assertSocketPathLength(socketPath string) error {
	if len(socketPath) >= maxUnixSocketBytes {
		return fmt.Errorf("control socket path is too long (%d bytes; max %d); use a shorter absolute AGENTHUB_HOME under /tmp", len(socketPath), maxUnixSocketBytes)
	}
	return nil
}

func loopbackURL(raw string) error {
	u, err := url.Parse(raw)
	if err != nil {
		return fmt.Errorf("upstream URL: %w", err)
	}
	if u.Scheme != "http" && u.Scheme != "https" {
		return fmt.Errorf("upstream URL must be http(s)")
	}
	if u.User != nil || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || strings.Contains(raw, "#") {
		return fmt.Errorf("loopback upstream URL contains disallowed components")
	}
	host := u.Hostname()
	if strings.EqualFold(host, "localhost") {
		return nil
	}
	ip := net.ParseIP(host)
	if ip == nil || !ip.IsLoopback() {
		return fmt.Errorf("upstream host must be loopback")
	}
	return nil
}

type upstreamTargetPolicy struct {
	Target          string
	CredentialClass string
	Transport       string
	Auth            string
	Surfaces        map[string]struct{}
	Host            string
	BasePath        string
	FinalPath       string
	LocalEndpoint   string
	ExternalAllowed bool
}

var upstreamTargetPolicies = map[string]upstreamTargetPolicy{
	upstreamTargetAnthropicAPI: {
		Target: upstreamTargetAnthropicAPI, CredentialClass: credentialClassAPIKey,
		Transport: transportAnthropicMessages, Auth: authAPIKey,
		Surfaces: map[string]struct{}{surfaceMessages: {}},
		Host:     "api.anthropic.com", BasePath: "/v1", FinalPath: "/v1/messages", LocalEndpoint: "/v1/messages",
		ExternalAllowed: true,
	},
	upstreamTargetOpenAIAPI: {
		Target: upstreamTargetOpenAIAPI, CredentialClass: credentialClassAPIKey,
		Transport: transportOpenAIChatCompletions, Auth: authBearer,
		Surfaces: map[string]struct{}{surfaceResponses: {}, surfaceChatCompletions: {}},
		Host:     "api.openai.com", BasePath: "/v1", FinalPath: "/v1/chat/completions", LocalEndpoint: "/v1/chat/completions",
		ExternalAllowed: true,
	},
	upstreamTargetKimiCodeMembership: {
		Target: upstreamTargetKimiCodeMembership, CredentialClass: credentialClassAPIKey,
		Transport: transportOpenAIChatCompletions, Auth: authBearer,
		Surfaces: map[string]struct{}{surfaceResponses: {}, surfaceChatCompletions: {}},
		Host:     "api.kimi.com", BasePath: "/coding/v1", FinalPath: "/coding/v1/chat/completions", LocalEndpoint: "/v1/chat/completions",
		ExternalAllowed: true,
	},
	upstreamTargetCodexChatGPTSubscription: {
		Target: upstreamTargetCodexChatGPTSubscription, CredentialClass: credentialClassOfficialLogin,
		Transport: transportCodexResponses, Auth: authBearer,
		Surfaces: map[string]struct{}{surfaceResponses: {}},
		Host:     "chatgpt.com", BasePath: "/backend-api/codex", FinalPath: "/backend-api/codex/responses", LocalEndpoint: "/v1/responses",
		ExternalAllowed: true,
	},
	upstreamTargetGrokXAISubscription: {
		Target: upstreamTargetGrokXAISubscription, CredentialClass: credentialClassOfficialLogin,
		Transport: transportGrokResponses, Auth: authBearer,
		Surfaces: map[string]struct{}{surfaceResponses: {}},
		Host:     "cli-chat-proxy.grok.com", BasePath: "/v1", FinalPath: "/v1/responses", LocalEndpoint: "/v1/responses",
		ExternalAllowed: true,
	},
}

func validateRuntimeUpstreamURL(raw, surface, transport, auth, target, credentialClass string) error {
	if err := loopbackURL(raw); err == nil {
		return validateLoopbackTargetMetadata(surface, transport, auth, target, credentialClass)
	}
	policy, err := requireUpstreamTargetPolicy(surface, transport, auth, target, credentialClass)
	if err != nil {
		return err
	}
	if !policy.ExternalAllowed {
		return fmt.Errorf("external upstream target is not allowed")
	}
	return validateExternalTargetBase(raw, policy)
}

func validateLoopbackTargetMetadata(surface, transport, auth, target, credentialClass string) error {
	if target == "" && credentialClass == "" {
		return nil
	}
	if target == upstreamTargetLoopback && credentialClass == credentialClassLocal {
		return nil
	}
	return fmt.Errorf("loopback upstream target identity is not allowed")
}

func requireUpstreamTargetPolicy(surface, transport, auth, target, credentialClass string) (upstreamTargetPolicy, error) {
	if target == "" || credentialClass == "" {
		return upstreamTargetPolicy{}, fmt.Errorf("external upstream target identity is incomplete")
	}
	policy, ok := upstreamTargetPolicies[target]
	if !ok || policy.CredentialClass != credentialClass || policy.Transport != transport || policy.Auth != auth {
		return upstreamTargetPolicy{}, fmt.Errorf("upstream target identity does not match transport or authentication")
	}
	if surface != "" {
		if _, ok := policy.Surfaces[surface]; !ok {
			return upstreamTargetPolicy{}, fmt.Errorf("upstream target does not match surface")
		}
	}
	return policy, nil
}

func validateExternalTargetBase(raw string, policy upstreamTargetPolicy) error {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || !u.IsAbs() || u.Host == "" {
		return fmt.Errorf("external upstream URL is invalid")
	}
	if u.Scheme != "https" ||
		(!strings.EqualFold(u.Host, policy.Host) && !strings.EqualFold(u.Host, policy.Host+":443")) {
		return fmt.Errorf("external upstream authority is not allowed")
	}
	if u.User != nil || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || u.RawFragment != "" ||
		strings.Contains(raw, "#") || u.EscapedPath() != u.Path {
		return fmt.Errorf("external upstream URL contains disallowed components")
	}
	if u.Path != policy.BasePath {
		return fmt.Errorf("external upstream base path is not allowed")
	}
	return nil
}

func buildFinalUpstreamURL(base, endpoint, transport, auth, target, credentialClass string) (string, error) {
	if err := loopbackURL(base); err == nil {
		if metadataErr := validateLoopbackTargetMetadata("", transport, auth, target, credentialClass); metadataErr != nil {
			return "", metadataErr
		}
		joined, err := joinLoopbackUpstreamPath(base, endpoint)
		if err != nil {
			return "", err
		}
		if err := loopbackURL(joined); err != nil {
			return "", err
		}
		return joined, nil
	}

	policy, err := requireUpstreamTargetPolicy("", transport, auth, target, credentialClass)
	if err != nil {
		return "", err
	}
	if !policy.ExternalAllowed {
		return "", fmt.Errorf("external upstream target is not allowed")
	}
	if err := validateExternalTargetBase(base, policy); err != nil {
		return "", err
	}
	if endpoint != policy.LocalEndpoint {
		return "", fmt.Errorf("upstream endpoint does not match target")
	}
	final := (&url.URL{Scheme: "https", Host: policy.Host, Path: policy.FinalPath}).String()
	if err := validateFinalUpstreamURL(final, policy); err != nil {
		return "", err
	}
	return final, nil
}

func validateFinalUpstreamURL(raw string, policy upstreamTargetPolicy) error {
	u, err := url.Parse(raw)
	if err != nil || !u.IsAbs() || u.Scheme != "https" || u.Host != policy.Host || u.Path != policy.FinalPath {
		return fmt.Errorf("final external upstream URL is not allowed")
	}
	if u.User != nil || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || u.RawFragment != "" ||
		u.EscapedPath() != u.Path {
		return fmt.Errorf("final external upstream URL contains disallowed components")
	}
	return nil
}

func joinLoopbackUpstreamPath(base, endpoint string) (string, error) {
	u, err := url.Parse(base)
	if err != nil {
		return "", fmt.Errorf("upstream URL: %w", err)
	}
	basePath := strings.TrimRight(u.Path, "/")
	endpointPath := "/" + strings.TrimLeft(endpoint, "/")
	if strings.HasSuffix(basePath, "/v1") && strings.HasPrefix(endpointPath, "/v1/") {
		endpointPath = strings.TrimPrefix(endpointPath, "/v1")
	}
	u.Path = basePath + endpointPath
	u.RawPath = ""
	return u.String(), nil
}

func isLoopbackRemote(remoteAddr string) bool {
	host, _, err := net.SplitHostPort(remoteAddr)
	if err != nil {
		host = remoteAddr
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}
