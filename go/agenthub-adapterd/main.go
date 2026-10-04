package main

import (
	"bytes"
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"strconv"
	"syscall"
	"time"
)

const runtimeConfigRejectedMessage = "agenthub-adapterd: runtime config rejected"

const maxConsecutiveRuntimeConfigRejections = 8

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
	runtimeConfigStdin := fs.Bool("runtime-config-stdin", false, "read one route-config.v0-isolated JSON value from stdin")
	runtimeConfigStdinStream := fs.Bool("runtime-config-stdin-stream", false, "read length-framed route configs from stdin and accept atomic updates")
	fs.Usage = func() {
		fmt.Fprintf(os.Stderr, "Usage: agenthub-adapterd run --home DIR --listen-port PORT [--runtime-config-stdin | --runtime-config-stdin-stream]\n")
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
	if *runtimeConfigStdin && *runtimeConfigStdinStream {
		fmt.Fprintln(os.Stderr, "agenthub-adapterd: choose one runtime config stdin mode")
		os.Exit(2)
	}

	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	var runtimeConfig *RuntimeConfig
	var runtimeConfigHash string
	if *runtimeConfigStdin {
		var rejection string
		runtimeConfig, runtimeConfigHash, rejection = loadRuntimeConfigForRun(os.Stdin)
		if rejection != "" {
			fmt.Fprintln(os.Stderr, rejection)
			os.Exit(1)
		}
	} else if *runtimeConfigStdinStream {
		var rejection string
		runtimeConfig, _, runtimeConfigHash, rejection = loadRuntimeConfigFrameForRun(os.Stdin)
		if rejection != "" {
			fmt.Fprintln(os.Stderr, rejection)
			os.Exit(1)
		}
	}

	rt, err := NewRuntime(*home, *listenPort, *controlSocket, cancel)
	if err != nil {
		fmt.Fprintf(os.Stderr, "agenthub-adapterd: %v\n", err)
		os.Exit(1)
	}
	if runtimeConfig != nil {
		if err := rt.SetRuntimeConfigWithDigest(runtimeConfig, runtimeConfigHash); err != nil {
			fmt.Fprintln(os.Stderr, "agenthub-adapterd: runtime config rejected")
			os.Exit(1)
		}
	}
	if *runtimeConfigStdinStream {
		go consumeRuntimeConfigFrames(ctx, rt, os.Stdin)
	}
	if err := rt.WritePID(); err != nil {
		fmt.Fprintf(os.Stderr, "agenthub-adapterd: pid file: %v\n", err)
		os.Exit(1)
	}

	fmt.Fprintf(os.Stdout, "agenthub-adapterd home: %s\n", rt.Home())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd control socket: %s\n", rt.ControlSocket())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd pid: %d\n", os.Getpid())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd log: %s\n", rt.LogFile())
	fmt.Fprintf(os.Stdout, "agenthub-adapterd messages port (inactive until Start): %d\n", *listenPort)

	if err := rt.ServeControl(ctx); err != nil && ctx.Err() == nil {
		fmt.Fprintf(os.Stderr, "agenthub-adapterd control: %v\n", err)
		os.Exit(1)
	}
	_ = rt.Shutdown(context.Background())
}

func loadRuntimeConfigFrameForRun(r io.Reader) (*RuntimeConfig, []byte, string, string) {
	config, raw, digest, err := readRuntimeConfigFrame(r)
	if err != nil {
		return nil, nil, "", runtimeConfigRejectedMessage
	}
	return config, raw, digest, ""
}

func consumeRuntimeConfigFrames(ctx context.Context, rt *Runtime, r io.Reader) {
	consecutiveRejections := 0
	for {
		config, _, digest, err := readRuntimeConfigFrame(r)
		if ctx.Err() != nil {
			return
		}
		if errors.Is(err, errRuntimeConfigFrameInvalid) {
			consecutiveRejections++
			rt.recordRuntimeConfigRejection()
			if consecutiveRejections >= maxConsecutiveRuntimeConfigRejections {
				rt.stopAfterRuntimeConfigStreamFailure()
				return
			}
			time.Sleep(time.Duration(consecutiveRejections) * 25 * time.Millisecond)
			continue
		}
		if err != nil {
			rt.stopAfterRuntimeConfigStreamFailure()
			return
		}
		if err := rt.SwapRuntimeConfig(config, digest); err != nil {
			consecutiveRejections++
			rt.recordRuntimeConfigRejection()
			if consecutiveRejections >= maxConsecutiveRuntimeConfigRejections {
				rt.stopAfterRuntimeConfigStreamFailure()
				return
			}
			time.Sleep(time.Duration(consecutiveRejections) * 25 * time.Millisecond)
			continue
		}
		consecutiveRejections = 0
		rt.logf("runtime config updated atomically")
	}
}

func loadRuntimeConfigForRun(r io.Reader) (*RuntimeConfig, string, string) {
	raw, err := io.ReadAll(io.LimitReader(r, maxRuntimeConfigSize+1))
	if err != nil || len(raw) > maxRuntimeConfigSize {
		return nil, "", runtimeConfigRejectedMessage
	}
	config, err := LoadRuntimeConfig(bytes.NewReader(raw))
	if err != nil {
		return nil, "", runtimeConfigRejectedMessage
	}
	return config, runtimeConfigDigest(raw), ""
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
