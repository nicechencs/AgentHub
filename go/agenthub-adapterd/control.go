package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"os"
	"time"
)

func (rt *Runtime) ServeControl(ctx context.Context) error {
	ln, cleanup, err := rt.openControlListener()
	if err != nil {
		return err
	}
	defer cleanup()
	if rt.controlNetwork == "tcp4" {
		if _, err := fmt.Fprintf(os.Stdout, "agenthub-adapterd control listener: tcp4 %s\n", rt.controlAddress); err != nil {
			_ = ln.Close()
			return fmt.Errorf("report TCP control endpoint: %w", err)
		}
	}
	srv := &http.Server{
		Handler:           rt.controlHTTPHandler(),
		ReadHeaderTimeout: rt.httpPolicy.ServerReadHeaderTimeout,
		ReadTimeout:       rt.httpPolicy.ServerReadTimeout,
		WriteTimeout:      rt.httpPolicy.ControlWriteTimeout,
		IdleTimeout:       rt.httpPolicy.ServerIdleTimeout,
		MaxHeaderBytes:    rt.httpPolicy.MaxHeaderBytes,
	}
	go func() {
		<-ctx.Done()
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancel()
		_ = srv.Shutdown(shutdownCtx)
		_ = ln.Close()
		cleanup()
	}()
	rt.logf("control listener ready using %s", rt.controlNetwork)
	err = srv.Serve(ln)
	if err == http.ErrServerClosed {
		return nil
	}
	return err
}

func (rt *Runtime) controlHTTPHandler() http.Handler {
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
	return rt.authenticatedControlHandler(mux)
}

func (rt *Runtime) serveControlHTTP(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		w.Header().Set("Allow", "POST")
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	raw, err := readStrictRequestBody(w, r, rt.httpPolicy.ControlBodyBytes)
	if err != nil {
		if errors.Is(err, errBodyTooLarge) {
			http.Error(w, "request body too large", http.StatusRequestEntityTooLarge)
			return
		}
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
