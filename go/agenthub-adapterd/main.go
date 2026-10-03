package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"strconv"
	"syscall"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "mock-upstream" {
		fs := flag.NewFlagSet("mock-upstream", flag.ExitOnError)
		listen := fs.String("listen", "127.0.0.1:0", "loopback listen address for the fixture upstream")
		_ = fs.Parse(os.Args[2:])
		if err := runMockUpstream(*listen); err != nil {
			fmt.Fprintf(os.Stderr, "mock-upstream: %v\n", err)
			os.Exit(1)
		}
		return
	}

	args := os.Args[1:]
	if len(args) > 0 && args[0] == "run" {
		args = args[1:]
	}
	fs := flag.NewFlagSet("agenthub-adapterd", flag.ExitOnError)
	home := fs.String("home", os.Getenv("AGENTHUB_HOME"), "absolute scratch AGENTHUB_HOME")
	listenPort := fs.Int("listen-port", envInt("AGENTHUB_ADAPTERD_LISTEN_PORT", 0), "loopback Messages port (0 = ephemeral; not the product default)")
	controlSocket := fs.String("control-socket", os.Getenv("AGENTHUB_ADAPTERD_CONTROL_SOCKET"), "absolute unix control socket (default $AGENTHUB_HOME/run/adapterd.sock)")
	fs.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: agenthub-adapterd run --home DIR --listen-port PORT\n")
		fmt.Fprintf(os.Stderr, "       agenthub-adapterd mock-upstream --listen 127.0.0.1:PORT\n")
		fs.PrintDefaults()
	}
	if err := fs.Parse(args); err != nil {
		os.Exit(2)
	}
	if *home == "" {
		fs.Usage()
		os.Exit(2)
	}

	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	rt, err := NewRuntime(*home, *listenPort, *controlSocket, cancel)
	if err != nil {
		fmt.Fprintf(os.Stderr, "agenthub-adapterd: %v\n", err)
		os.Exit(1)
	}
	if err := rt.WritePID(); err != nil {
		fmt.Fprintf(os.Stderr, "agenthub-adapterd: pid file: %v\n", err)
		os.Exit(1)
	}

	fmt.Fprintf(os.Stdout, "agenthub-adapterd home: %s\n", rt.Home())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd control socket: %s\n", rt.ControlSocket())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd pid: %d\n", os.Getpid())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd log: %s\n", rt.LogFile())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd messages port (inactive until CommitDesired or ActivateProbeListen): %d\n", *listenPort)

	if err := rt.ServeControl(ctx); err != nil && ctx.Err() == nil {
		fmt.Fprintf(os.Stderr, "agenthub-adapterd control: %v\n", err)
		os.Exit(1)
	}
	_ = rt.Shutdown(context.Background())
}

func envInt(name string, fallback int) int {
	raw := os.Getenv(name)
	if raw == "" {
		return fallback
	}
	n, err := strconv.Atoi(raw)
	if err != nil {
		return fallback
	}
	return n
}
