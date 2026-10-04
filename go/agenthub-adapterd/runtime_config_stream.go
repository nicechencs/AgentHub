package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"errors"
	"io"
)

const runtimeConfigFrameHeaderSize = 4

var errRuntimeConfigFrameInvalid = errors.New("runtime config frame is invalid")

// readRuntimeConfigFrame reads one private runtime-config frame. The returned
// payload is the exact byte sequence covered by the digest. Parse and schema
// failures are deliberately collapsed so attacker-controlled config fields and
// login information never become part of an error or log message.
func readRuntimeConfigFrame(r io.Reader) (*RuntimeConfig, []byte, string, error) {
	if r == nil {
		return nil, nil, "", errors.New("runtime config frame reader is unavailable")
	}

	var header [runtimeConfigFrameHeaderSize]byte
	if _, err := io.ReadFull(r, header[:]); err != nil {
		if errors.Is(err, io.EOF) {
			return nil, nil, "", io.EOF
		}
		return nil, nil, "", errors.New("runtime config frame header is truncated")
	}

	length := binary.BigEndian.Uint32(header[:])
	if length == 0 {
		return nil, nil, "", errRuntimeConfigFrameInvalid
	}
	if uint64(length) > uint64(maxRuntimeConfigSize) {
		return nil, nil, "", errors.New("runtime config frame exceeds the size limit")
	}

	raw := make([]byte, int(length))
	if _, err := io.ReadFull(r, raw); err != nil {
		return nil, nil, "", errors.New("runtime config frame payload is truncated")
	}
	config, err := LoadRuntimeConfig(bytes.NewReader(raw))
	if err != nil {
		return nil, nil, "", errRuntimeConfigFrameInvalid
	}
	return config, raw, runtimeConfigDigest(raw), nil
}

// writeRuntimeConfigFrame writes one frame without interpreting its payload.
// Keeping this as a byte-level helper lets probes exercise receiver rejection
// paths without adding a second JSON encoder or validation implementation.
func writeRuntimeConfigFrame(w io.Writer, raw []byte) error {
	if w == nil {
		return errors.New("runtime config frame writer is unavailable")
	}
	if len(raw) == 0 {
		return errors.New("runtime config frame is invalid")
	}
	if len(raw) > maxRuntimeConfigSize {
		return errors.New("runtime config frame exceeds the size limit")
	}

	var header [runtimeConfigFrameHeaderSize]byte
	binary.BigEndian.PutUint32(header[:], uint32(len(raw)))
	if err := writeRuntimeConfigFrameBytes(w, header[:]); err != nil {
		return errors.New("runtime config frame header write failed")
	}
	if err := writeRuntimeConfigFrameBytes(w, raw); err != nil {
		return errors.New("runtime config frame payload write failed")
	}
	return nil
}

func writeRuntimeConfigFrameBytes(w io.Writer, raw []byte) error {
	for len(raw) > 0 {
		n, err := w.Write(raw)
		if err != nil {
			return err
		}
		if n <= 0 || n > len(raw) {
			return io.ErrShortWrite
		}
		raw = raw[n:]
	}
	return nil
}

func runtimeConfigDigest(raw []byte) string {
	digest := sha256.Sum256(raw)
	return hex.EncodeToString(digest[:])
}
