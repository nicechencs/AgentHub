package main

import (
	"context"
	"crypto/tls"
	"errors"
	"net"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"sync/atomic"
	"testing"
)

type resolverFunc func(context.Context, string, string) ([]netip.Addr, error)

func (fn resolverFunc) LookupNetIP(ctx context.Context, network, host string) ([]netip.Addr, error) {
	return fn(ctx, network, host)
}

type contextDialerFunc func(context.Context, string, string) (net.Conn, error)

func (fn contextDialerFunc) DialContext(ctx context.Context, network, address string) (net.Conn, error) {
	return fn(ctx, network, address)
}

func TestValidatedUpstreamDialPinsSinglePublicResolution(t *testing.T) {
	var lookups atomic.Int32
	var dialed string
	resolver := resolverFunc(func(_ context.Context, network, host string) ([]netip.Addr, error) {
		lookups.Add(1)
		if network != "ip" || host != "api.openai.com" {
			t.Fatalf("lookup network=%q host=%q", network, host)
		}
		return []netip.Addr{netip.MustParseAddr("8.8.8.8")}, nil
	})
	dialer := contextDialerFunc(func(_ context.Context, network, address string) (net.Conn, error) {
		if network != "tcp" {
			t.Fatalf("network=%q", network)
		}
		dialed = address
		client, server := net.Pipe()
		_ = server.Close()
		return client, nil
	})
	connection, err := validatedUpstreamDialContext(resolver, dialer)(context.Background(), "tcp", "api.openai.com:443")
	if err != nil {
		t.Fatal(err)
	}
	_ = connection.Close()
	if lookups.Load() != 1 || dialed != "8.8.8.8:443" {
		t.Fatalf("lookups=%d dialed=%q", lookups.Load(), dialed)
	}
}

func TestValidatedUpstreamDialRejectsMixedPublicPrivateWithoutDial(t *testing.T) {
	resolver := resolverFunc(func(context.Context, string, string) ([]netip.Addr, error) {
		return []netip.Addr{netip.MustParseAddr("8.8.8.8"), netip.MustParseAddr("10.0.0.8")}, nil
	})
	var dials atomic.Int32
	dialer := contextDialerFunc(func(context.Context, string, string) (net.Conn, error) {
		dials.Add(1)
		return nil, errors.New("must not dial")
	})
	_, err := validatedUpstreamDialContext(resolver, dialer)(context.Background(), "tcp", "api.openai.com:443")
	if err == nil || dials.Load() != 0 {
		t.Fatalf("err=%v dials=%d", err, dials.Load())
	}
}

func TestPublicUpstreamAddressRejectsSpecialNetworks(t *testing.T) {
	for _, raw := range []string{
		"0.0.0.0", "10.0.0.1", "100.64.0.1", "127.0.0.1", "169.254.169.254",
		"172.16.0.1", "192.0.2.1", "192.168.0.1", "198.18.0.1", "198.51.100.1",
		"203.0.113.1", "224.0.0.1", "240.0.0.1", "::", "::1", "::ffff:127.0.0.1",
		"64:ff9b::a9fe:a9fe", "100::1", "2001::1", "2001:2::1", "2001:db8::1", "2002::1", "3fff::1", "fc00::1", "fec0::1", "fe80::1", "ff00::1",
	} {
		if isPublicUpstreamAddress(netip.MustParseAddr(raw)) {
			t.Errorf("special address %s was accepted", raw)
		}
	}
	for _, raw := range []string{"8.8.8.8", "1.1.1.1", "2606:4700:4700::1111"} {
		if !isPublicUpstreamAddress(netip.MustParseAddr(raw)) {
			t.Errorf("public address %s was rejected", raw)
		}
	}
}

func TestValidatedUpstreamDialResolvesOnlyOpenedExactHosts(t *testing.T) {
	var lookups atomic.Int32
	resolver := resolverFunc(func(context.Context, string, string) ([]netip.Addr, error) {
		lookups.Add(1)
		return []netip.Addr{netip.MustParseAddr("8.8.8.8")}, nil
	})
	dialer := contextDialerFunc(func(context.Context, string, string) (net.Conn, error) {
		return nil, errors.New("must not dial")
	})
	validated := validatedUpstreamDialContext(resolver, dialer)
	for _, address := range []string{
		"chatgpt.com:443",
		"cli-chat-proxy.grok.com:443",
		"api.openai.com.evil.example:443",
		"api.openai.com:80",
		"8.8.8.8:443",
	} {
		if _, err := validated(context.Background(), "tcp", address); err == nil {
			t.Errorf("address %q was accepted", address)
		}
	}
	if lookups.Load() != 2 {
		t.Fatalf("resolver lookups=%d, want the two opened official-login hosts", lookups.Load())
	}
}

func TestUpstreamClientDisablesEnvironmentProxyAndPreservesTLSServerName(t *testing.T) {
	serverName := make(chan string, 1)
	server := httptest.NewUnstartedServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	}))
	server.TLS = &tls.Config{GetConfigForClient: func(hello *tls.ClientHelloInfo) (*tls.Config, error) {
		serverName <- hello.ServerName
		return nil, nil
	}}
	server.StartTLS()
	t.Cleanup(server.Close)

	_, serverPort, err := net.SplitHostPort(server.Listener.Addr().String())
	if err != nil {
		t.Fatal(err)
	}
	resolver := resolverFunc(func(_ context.Context, _, host string) ([]netip.Addr, error) {
		if host != "api.openai.com" {
			t.Fatalf("resolved unexpected host %q", host)
		}
		return []netip.Addr{netip.MustParseAddr("8.8.8.8")}, nil
	})
	var dialed string
	dialer := contextDialerFunc(func(ctx context.Context, network, address string) (net.Conn, error) {
		dialed = address
		return (&net.Dialer{}).DialContext(ctx, network, net.JoinHostPort("127.0.0.1", serverPort))
	})
	t.Setenv("HTTP_PROXY", "http://127.0.0.1:1")
	t.Setenv("HTTPS_PROXY", "http://127.0.0.1:1")
	client := newUpstreamHTTPClientWithNetwork(defaultRouteHTTPSafetyPolicy, resolver, dialer)
	transport := client.Transport.(*http.Transport)
	if transport.Proxy != nil {
		t.Fatal("upstream client inherited an environment proxy")
	}
	transport.TLSClientConfig = &tls.Config{InsecureSkipVerify: true} // test server certificate only
	response, err := client.Get("https://api.openai.com/v1/chat/completions")
	if err != nil {
		t.Fatal(err)
	}
	_ = response.Body.Close()
	if dialed != "8.8.8.8:443" {
		t.Fatalf("dialed=%q", dialed)
	}
	select {
	case got := <-serverName:
		if got != "api.openai.com" {
			t.Fatalf("TLS SNI=%q", got)
		}
	default:
		t.Fatal("TLS server did not observe SNI")
	}
}
