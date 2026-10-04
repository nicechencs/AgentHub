package main

import (
	"encoding/json"
	"strings"
	"sync"
)

const grokOfficialAffinityEntries = 1024

type grokOfficialAffinity struct {
	mu      sync.Mutex
	members map[string]string
	order   []string
}

func newGrokOfficialAffinity() *grokOfficialAffinity {
	return &grokOfficialAffinity{members: make(map[string]string)}
}

func (affinity *grokOfficialAffinity) lookup(responseID string) (string, bool) {
	if affinity == nil || strings.TrimSpace(responseID) == "" {
		return "", false
	}
	affinity.mu.Lock()
	defer affinity.mu.Unlock()
	memberID, ok := affinity.members[strings.TrimSpace(responseID)]
	return memberID, ok
}

func (affinity *grokOfficialAffinity) store(responseID, memberID string) {
	responseID = strings.TrimSpace(responseID)
	memberID = strings.TrimSpace(memberID)
	if affinity == nil || responseID == "" || memberID == "" {
		return
	}
	affinity.mu.Lock()
	defer affinity.mu.Unlock()
	if _, exists := affinity.members[responseID]; !exists {
		if len(affinity.members) >= grokOfficialAffinityEntries && len(affinity.order) > 0 {
			delete(affinity.members, affinity.order[0])
			affinity.order = affinity.order[1:]
		}
		affinity.order = append(affinity.order, responseID)
	}
	affinity.members[responseID] = memberID
}

func grokOfficialResponseID(value map[string]any) string {
	responseID, _ := value["id"].(string)
	return strings.TrimSpace(responseID)
}

func grokOfficialResponseIDFromSSE(raw []byte) string {
	var responseID string
	terminal := false
	for _, frame := range strings.Split(strings.ReplaceAll(string(raw), "\r\n", "\n"), "\n\n") {
		kind, hasData, err := parseGrokResponsesFrame([]byte(frame))
		if err != nil {
			return ""
		}
		if !hasData {
			continue
		}
		if terminal {
			return ""
		}
		if kind == "response.failed" || kind == "error" {
			return ""
		}
		var data []string
		for _, line := range strings.Split(frame, "\n") {
			if value, ok := strings.CutPrefix(line, "data:"); ok {
				data = append(data, strings.TrimSpace(value))
			}
		}
		var event map[string]any
		if json.Unmarshal([]byte(strings.Join(data, "\n")), &event) != nil {
			return ""
		}
		if kind == "response.completed" {
			terminal = true
			if response, ok := event["response"].(map[string]any); ok {
				responseID = grokOfficialResponseID(response)
			}
		} else if kind == "response.incomplete" {
			terminal = true
			responseID = ""
		}
	}
	if !terminal {
		return ""
	}
	return responseID
}
