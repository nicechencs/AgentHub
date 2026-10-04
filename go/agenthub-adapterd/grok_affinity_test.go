package main

import (
	"strconv"
	"testing"
	"time"
)

func TestOfficialAffinityUsesLRUEviction(t *testing.T) {
	affinity := newGrokOfficialAffinity()
	base := time.Unix(1_700_000_000, 0)
	affinity.storeAt("seed:", "active", "account:active", base)
	for index := 0; index < grokOfficialAffinityEntries-1; index++ {
		affinity.storeAt("response:", strconv.Itoa(index), "account:other", base)
	}
	if _, ok := affinity.lookupAt("seed:", "active", base.Add(time.Minute)); !ok {
		t.Fatal("active seed disappeared before capacity eviction")
	}
	affinity.storeAt("response:", "new", "account:new", base.Add(2*time.Minute))
	if memberID, ok := affinity.lookupAt("seed:", "active", base.Add(3*time.Minute)); !ok || memberID != "account:active" {
		t.Fatalf("active seed was evicted: member=%q ok=%v", memberID, ok)
	}
	if _, ok := affinity.lookupAt("response:", "0", base.Add(3*time.Minute)); ok {
		t.Fatal("least recently used response was not evicted")
	}
}

func TestOfficialAffinityRefreshesAndExpiresIdleEntries(t *testing.T) {
	affinity := newGrokOfficialAffinity()
	base := time.Unix(1_700_000_000, 0)
	affinity.storeAt("seed:", "session", "account:first", base)
	affinity.storeAt("seed:", "session", "account:updated", base.Add(time.Hour))
	if memberID, ok := affinity.lookupAt("seed:", "session", base.Add(2*time.Hour+30*time.Minute)); !ok || memberID != "account:updated" {
		t.Fatalf("refreshed entry expired early: member=%q ok=%v", memberID, ok)
	}
	if _, ok := affinity.lookupAt("seed:", "session", base.Add(5*time.Hour)); ok {
		t.Fatal("idle entry did not expire")
	}
}
