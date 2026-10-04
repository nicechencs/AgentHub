package main

import (
	"context"
	"fmt"
	"net"
	"net/netip"
	"strconv"
	"strings"
)

type upstreamIPResolver interface {
	LookupNetIP(context.Context, string, string) ([]netip.Addr, error)
}

type upstreamContextDialer interface {
	DialContext(context.Context, string, string) (net.Conn, error)
}

var forbiddenExternalPrefixes = []netip.Prefix{
	netip.MustParsePrefix("0.0.0.0/8"),
	netip.MustParsePrefix("10.0.0.0/8"),
	netip.MustParsePrefix("100.64.0.0/10"),
	netip.MustParsePrefix("127.0.0.0/8"),
	netip.MustParsePrefix("169.254.0.0/16"),
	netip.MustParsePrefix("172.16.0.0/12"),
	netip.MustParsePrefix("192.0.0.0/24"),
	netip.MustParsePrefix("192.0.2.0/24"),
	netip.MustParsePrefix("192.88.99.0/24"),
	netip.MustParsePrefix("192.168.0.0/16"),
	netip.MustParsePrefix("198.18.0.0/15"),
	netip.MustParsePrefix("198.51.100.0/24"),
	netip.MustParsePrefix("203.0.113.0/24"),
	netip.MustParsePrefix("224.0.0.0/4"),
	netip.MustParsePrefix("240.0.0.0/4"),
	netip.MustParsePrefix("64:ff9b::/96"),
	netip.MustParsePrefix("64:ff9b:1::/48"),
	netip.MustParsePrefix("100::/64"),
	netip.MustParsePrefix("2001::/23"),
	netip.MustParsePrefix("2001:db8::/32"),
	netip.MustParsePrefix("2002::/16"),
	netip.MustParsePrefix("3fff::/20"),
	netip.MustParsePrefix("fc00::/7"),
	netip.MustParsePrefix("fec0::/10"),
	netip.MustParsePrefix("fe80::/10"),
	netip.MustParsePrefix("ff00::/8"),
}

func validatedUpstreamDialContext(resolver upstreamIPResolver, dialer upstreamContextDialer) func(context.Context, string, string) (net.Conn, error) {
	return func(ctx context.Context, network, address string) (net.Conn, error) {
		if network != "tcp" && network != "tcp4" && network != "tcp6" {
			return nil, fmt.Errorf("upstream dial network is invalid")
		}
		host, port, err := net.SplitHostPort(address)
		if err != nil || host == "" {
			return nil, fmt.Errorf("upstream dial address is invalid")
		}
		portNumber, err := strconv.Atoi(port)
		if err != nil || portNumber < 1 || portNumber > 65535 {
			return nil, fmt.Errorf("upstream dial port is invalid")
		}

		if literal, parseErr := netip.ParseAddr(host); parseErr == nil {
			literal = literal.Unmap()
			if !literal.IsLoopback() {
				return nil, fmt.Errorf("external upstream must use an approved hostname")
			}
			return dialer.DialContext(ctx, network, net.JoinHostPort(literal.String(), port))
		}

		loopbackOnly := strings.EqualFold(host, "localhost")
		if !loopbackOnly && !isApprovedExternalHostname(host) {
			return nil, fmt.Errorf("upstream hostname is not allowed")
		}
		if !loopbackOnly && portNumber != 443 {
			return nil, fmt.Errorf("external upstream port is not allowed")
		}
		addresses, err := resolver.LookupNetIP(ctx, "ip", host)
		if err != nil || len(addresses) == 0 {
			return nil, fmt.Errorf("upstream hostname could not be resolved")
		}
		validated := make([]netip.Addr, 0, len(addresses))
		for _, raw := range addresses {
			if !raw.IsValid() {
				return nil, fmt.Errorf("upstream hostname resolved to an invalid address")
			}
			address := raw.Unmap()
			if (loopbackOnly && !address.IsLoopback()) || (!loopbackOnly && !isPublicUpstreamAddress(address)) {
				return nil, fmt.Errorf("upstream hostname resolved outside its allowed network scope")
			}
			validated = append(validated, address)
		}

		var lastErr error
		for _, address := range validated {
			connection, dialErr := dialer.DialContext(ctx, network, net.JoinHostPort(address.String(), port))
			if dialErr == nil {
				return connection, nil
			}
			lastErr = dialErr
			if ctx.Err() != nil {
				return nil, ctx.Err()
			}
		}
		return nil, fmt.Errorf("upstream connection failed: %w", lastErr)
	}
}

func isApprovedExternalHostname(host string) bool {
	for _, policy := range upstreamTargetPolicies {
		if policy.ExternalAllowed && strings.EqualFold(host, policy.Host) {
			return true
		}
	}
	return false
}

func isPublicUpstreamAddress(address netip.Addr) bool {
	address = address.Unmap()
	if !address.IsValid() || !address.IsGlobalUnicast() || address.IsPrivate() || address.IsLoopback() ||
		address.IsLinkLocalUnicast() || address.IsLinkLocalMulticast() || address.IsMulticast() || address.IsUnspecified() {
		return false
	}
	for _, prefix := range forbiddenExternalPrefixes {
		if prefix.Contains(address) {
			return false
		}
	}
	return true
}
