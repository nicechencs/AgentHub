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

func validateRuntimeUpstreamURL(raw, transport string) error {
	if err := loopbackURL(raw); err == nil {
		return nil
	}
	if transport != transportAnthropicMessages {
		return fmt.Errorf("external upstream transport is not allowed")
	}
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil {
		return fmt.Errorf("upstream URL: %w", err)
	}
	if u.Scheme != "https" ||
		(!strings.EqualFold(u.Host, "api.anthropic.com") && !strings.EqualFold(u.Host, "api.anthropic.com:443")) {
		return fmt.Errorf("external upstream must be official Anthropic HTTPS")
	}
	if u.User != nil || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || strings.Contains(raw, "#") {
		return fmt.Errorf("external upstream URL contains disallowed components")
	}
	if u.EscapedPath() != u.Path || (u.Path != "" && u.Path != "/" && u.Path != "/v1") {
		return fmt.Errorf("external upstream path is not allowed")
	}
	return nil
}

// validateFinalUpstreamURL repeats the trust-boundary check after the fixed API
// path has been joined. It intentionally accepts no host or transport beyond
// validateRuntimeUpstreamURL's current allowlist.
func validateFinalUpstreamURL(raw, transport string) error {
	u, err := url.Parse(strings.TrimSpace(raw))
	if err != nil || !u.IsAbs() || u.Host == "" {
		return fmt.Errorf("final upstream URL is invalid")
	}
	if u.User != nil || u.RawQuery != "" || u.ForceQuery || u.Fragment != "" || strings.Contains(raw, "#") || u.EscapedPath() != u.Path {
		return fmt.Errorf("final upstream URL contains disallowed components")
	}
	if err := loopbackURL(raw); err == nil {
		return nil
	}
	if transport != transportAnthropicMessages || u.Scheme != "https" ||
		(!strings.EqualFold(u.Host, "api.anthropic.com") && !strings.EqualFold(u.Host, "api.anthropic.com:443")) ||
		u.Path != "/v1/messages" {
		return fmt.Errorf("final external upstream URL is not allowed")
	}
	return nil
}

func isLoopbackRemote(remoteAddr string) bool {
	host, _, err := net.SplitHostPort(remoteAddr)
	if err != nil {
		host = remoteAddr
	}
	ip := net.ParseIP(host)
	return ip != nil && ip.IsLoopback()
}
