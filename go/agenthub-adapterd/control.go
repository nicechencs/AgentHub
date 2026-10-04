package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"time"
)

func (rt *Runtime) ServeControl(ctx context.Context) error {
	if err := os.RemoveAll(rt.controlSocket); err != nil {
		return fmt.Errorf("remove old control socket: %w", err)
	}
	ln, err := net.Listen("unix", rt.controlSocket)
	if err != nil {
		return fmt.Errorf("listen control socket: %w", err)
	}
	if err := os.Chmod(rt.controlSocket, 0o600); err != nil {
		_ = ln.Close()
		return fmt.Errorf("chmod control socket: %w", err)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/control", rt.serveControlHTTP)
	mux.HandleFunc("/healthz", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(fmt.Sprintf(`{"ok":true,"pid":%d}`, os.Getpid())))
	})
	srv := &http.Server{
		Handler:           mux,
		ReadHeaderTimeout: 10 * time.Second,
	}
	go func() {
		<-ctx.Done()
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutdownCtx)
		_ = ln.Close()
		_ = os.Remove(rt.controlSocket)
	}()
	rt.logf("control socket listening at %s", rt.controlSocket)
	err = srv.Serve(ln)
	if err == http.ErrServerClosed {
		return nil
	}
	return err
}

func (rt *Runtime) serveControlHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	raw, err := io.ReadAll(io.LimitReader(r.Body, 1<<20))
	if err != nil {
		http.Error(w, "unable to read body", http.StatusBadRequest)
		return
	}
	reply := rt.HandleControlContext(r.Context(), raw)
	w.Header().Set("Content-Type", "application/json")
	if !reply.OK {
		w.WriteHeader(http.StatusBadRequest)
	}
	if err := json.NewEncoder(w).Encode(reply); err != nil {
		rt.logf("write control reply: %v", err)
	}
}
