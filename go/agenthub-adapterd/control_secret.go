package main

import (
	"encoding/binary"
	"errors"
	"io"
	"os"
)

const (
	controlTokenEnvironment   = "AGENTHUB_ADAPTERD_CONTROL_TOKEN"
	controlTokenEncodedLength = 43
)

var errControlTokenSource = errors.New("control authentication source rejected")

func rejectLegacyControlTokenEnvironment() bool {
	_, present := os.LookupEnv(controlTokenEnvironment)
	if present {
		_ = os.Unsetenv(controlTokenEnvironment)
	}
	return present
}

func consumeControlTokenStdin(tcpControl, tokenStdin bool, reader io.Reader) (string, error) {
	if tcpControl != tokenStdin {
		return "", errControlTokenSource
	}
	if !tcpControl {
		return "", nil
	}
	return readControlTokenPrelude(reader)
}

func readControlTokenPrelude(reader io.Reader) (string, error) {
	if reader == nil {
		return "", errControlTokenSource
	}
	var header [4]byte
	if _, err := io.ReadFull(reader, header[:]); err != nil {
		return "", errControlTokenSource
	}
	if binary.BigEndian.Uint32(header[:]) != controlTokenEncodedLength {
		return "", errControlTokenSource
	}
	raw := make([]byte, controlTokenEncodedLength)
	if _, err := io.ReadFull(reader, raw); err != nil {
		return "", errControlTokenSource
	}
	token := string(raw)
	if validateControlToken(token) != nil {
		return "", errControlTokenSource
	}
	return token, nil
}
