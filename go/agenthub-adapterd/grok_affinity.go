package main

import (
	"container/list"
	"encoding/json"
	"strings"
	"sync"
	"time"
)

const (
	grokOfficialAffinityEntries = 8192
	grokOfficialAffinityIdleTTL = 2 * time.Hour
)

type grokOfficialAffinityEntry struct {
	memberID string
	lastUsed time.Time
	element  *list.Element
}

type grokOfficialAffinity struct {
	mu      sync.Mutex
	members map[string]*grokOfficialAffinityEntry
	order   *list.List
}

func newGrokOfficialAffinity() *grokOfficialAffinity {
	return &grokOfficialAffinity{members: make(map[string]*grokOfficialAffinityEntry), order: list.New()}
}

func (affinity *grokOfficialAffinity) lookupResponse(responseID string) (string, bool) {
	return affinity.lookup("response:", responseID)
}

func (affinity *grokOfficialAffinity) lookupSeed(seed string) (string, bool) {
	return affinity.lookup("seed:", seed)
}

func (affinity *grokOfficialAffinity) lookup(prefix, value string) (string, bool) {
	return affinity.lookupAt(prefix, value, time.Now())
}

func (affinity *grokOfficialAffinity) lookupAt(prefix, value string, now time.Time) (string, bool) {
	value = strings.TrimSpace(value)
	if affinity == nil || value == "" {
		return "", false
	}
	affinity.mu.Lock()
	defer affinity.mu.Unlock()
	affinity.pruneExpiredLocked(now)
	entry, ok := affinity.members[prefix+value]
	if !ok {
		return "", false
	}
	entry.lastUsed = now
	affinity.order.MoveToBack(entry.element)
	return entry.memberID, true
}

func (affinity *grokOfficialAffinity) storeResponse(responseID, memberID string) {
	affinity.store("response:", responseID, memberID)
}

func (affinity *grokOfficialAffinity) storeSeed(seed, memberID string) {
	affinity.store("seed:", seed, memberID)
}

func (affinity *grokOfficialAffinity) store(prefix, value, memberID string) {
	affinity.storeAt(prefix, value, memberID, time.Now())
}

func (affinity *grokOfficialAffinity) storeAt(prefix, value, memberID string, now time.Time) {
	value = strings.TrimSpace(value)
	memberID = strings.TrimSpace(memberID)
	if affinity == nil || value == "" || memberID == "" {
		return
	}
	key := prefix + value
	affinity.mu.Lock()
	defer affinity.mu.Unlock()
	affinity.pruneExpiredLocked(now)
	if entry, exists := affinity.members[key]; exists {
		entry.memberID = memberID
		entry.lastUsed = now
		affinity.order.MoveToBack(entry.element)
		return
	}
	for len(affinity.members) >= grokOfficialAffinityEntries {
		affinity.removeOldestLocked()
	}
	element := affinity.order.PushBack(key)
	affinity.members[key] = &grokOfficialAffinityEntry{memberID: memberID, lastUsed: now, element: element}
}

func (affinity *grokOfficialAffinity) pruneExpiredLocked(now time.Time) {
	cutoff := now.Add(-grokOfficialAffinityIdleTTL)
	for {
		front := affinity.order.Front()
		if front == nil {
			return
		}
		key, _ := front.Value.(string)
		entry := affinity.members[key]
		if entry != nil && entry.lastUsed.After(cutoff) {
			return
		}
		affinity.order.Remove(front)
		delete(affinity.members, key)
	}
}

func (affinity *grokOfficialAffinity) removeOldestLocked() {
	front := affinity.order.Front()
	if front == nil {
		return
	}
	key, _ := front.Value.(string)
	affinity.order.Remove(front)
	delete(affinity.members, key)
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
