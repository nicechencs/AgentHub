package main

import (
	"sort"
	"strings"
	"sync"
	"time"
)

const (
	policyPriorityFailover = "priority_failover"
	policyRoundRobin       = "round_robin"

	classRequest     = "request"
	classAuth        = "auth"
	classEntitlement = "entitlement"
	classQuota       = "quota"
	classTransient   = "transient"

	defaultCooldown = 2 * time.Second
	quotaCooldown   = 15 * time.Minute
	maxCooldown     = time.Hour
)

type PoolMember struct {
	ID                string
	SourceKind        string
	SourceID          string
	RefreshKind       string
	UpstreamBaseURL   string
	UpstreamKey       string
	UpstreamAuth      string
	UpstreamTransport string
	Priority          int64
	Position          int64
	Models            []string
	QuotaRemainingPct *float64

	cooldownUntil time.Time
	modelCool     map[string]time.Time
	healthy       bool
}

func (m *PoolMember) clone() PoolMember {
	out := *m
	if m.Models != nil {
		out.Models = append([]string(nil), m.Models...)
	}
	if m.QuotaRemainingPct != nil {
		v := *m.QuotaRemainingPct
		out.QuotaRemainingPct = &v
	}
	if m.modelCool != nil {
		out.modelCool = make(map[string]time.Time, len(m.modelCool))
		for model, until := range m.modelCool {
			out.modelCool[model] = until
		}
	}
	return out
}

func (m *PoolMember) serves(model string, now time.Time) bool {
	if !m.healthy {
		return false
	}
	if now.Before(m.cooldownUntil) {
		return false
	}
	if model != "" && m.modelCool != nil {
		if until, ok := m.modelCool[model]; ok && now.Before(until) {
			return false
		}
	}
	if len(m.Models) == 0 {
		return false
	}
	for _, item := range m.Models {
		if item == model {
			return true
		}
	}
	return false
}

type PoolSnapshot struct {
	SchedulePolicy     string
	MemberCount        int
	HealthyMemberCount int
}

type Pool struct {
	mu           sync.Mutex
	policy       string
	fixtureModel string
	members      []*PoolMember
	rrCursors    map[string]int
}

func NewPoolFromFixture(fixture ProbeFixture) (*Pool, error) {
	policy := strings.TrimSpace(fixture.SchedulePolicy)
	if policy == "" {
		policy = policyPriorityFailover
	}
	if policy != policyPriorityFailover && policy != policyRoundRobin {
		return nil, errInvalidPolicy
	}
	model := strings.TrimSpace(fixture.FixtureModel)
	if model == "" {
		model = "claude-probe-fixture"
	}
	raw := fixture.Members
	if len(raw) == 0 {
		if strings.TrimSpace(fixture.UpstreamBaseURL) == "" {
			return nil, errIncompletePool
		}
		raw = []ProbeMember{{
			ID:              "legacy",
			UpstreamBaseURL: fixture.UpstreamBaseURL,
			Priority:        0,
			Position:        0,
			Models:          []string{model},
		}}
	}
	members := make([]*PoolMember, 0, len(raw))
	for i, item := range raw {
		id := strings.TrimSpace(item.ID)
		if id == "" {
			id = "member-" + itoa(i)
		}
		base := strings.TrimSpace(item.UpstreamBaseURL)
		if base == "" {
			return nil, errIncompletePool
		}
		if item.UpstreamTransport == "" {
			if err := loopbackURL(base); err != nil {
				return nil, err
			}
		} else if err := validateRuntimeUpstreamURL(base, item.UpstreamTransport); err != nil {
			return nil, err
		}
		models := append([]string(nil), item.Models...)
		if len(models) == 0 {
			models = []string{model}
		}
		var quota *float64
		if item.QuotaRemainingPct != nil {
			v := *item.QuotaRemainingPct
			quota = &v
		}
		auth := normalizedUpstreamAuth(item.UpstreamAuth)
		transport := item.UpstreamTransport
		if transport == "" && auth == authAPIKey {
			// Legacy probe.json had no transport field. Its x-api-key members
			// are Messages fixtures and still require the Anthropic version header.
			transport = transportAnthropicMessages
		}
		members = append(members, &PoolMember{
			ID:                id,
			SourceKind:        strings.TrimSpace(item.SourceKind),
			SourceID:          strings.TrimSpace(item.SourceID),
			RefreshKind:       normalizedRefreshKind(item.RefreshKind),
			UpstreamBaseURL:   strings.TrimRight(base, "/"),
			UpstreamKey:       item.UpstreamKey,
			UpstreamAuth:      auth,
			UpstreamTransport: transport,
			Priority:          item.Priority,
			Position:          item.Position,
			Models:            models,
			QuotaRemainingPct: quota,
			healthy:           true,
			modelCool:         map[string]time.Time{},
		})
	}
	return &Pool{
		policy:       policy,
		fixtureModel: model,
		members:      members,
		rrCursors:    map[string]int{},
	}, nil
}

func normalizedRefreshKind(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return refreshNone
	}
	return raw
}

func normalizedUpstreamAuth(raw string) string {
	if strings.TrimSpace(raw) == authAPIKey {
		return authAPIKey
	}
	return authBearer
}

var (
	errInvalidPolicy  = errString("schedule_policy must be priority_failover or round_robin")
	errIncompletePool = errString("pool members are incomplete")
)

type errString string

func (e errString) Error() string { return string(e) }

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var buf [16]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	return string(buf[i:])
}

func (p *Pool) FixtureModel() string {
	if p == nil {
		return ""
	}
	return p.fixtureModel
}

func (p *Pool) Policy() string {
	if p == nil {
		return ""
	}
	return p.policy
}

func (p *Pool) Secrets() []string {
	if p == nil {
		return nil
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	out := make([]string, 0, len(p.members))
	for _, m := range p.members {
		if m.UpstreamKey != "" {
			out = append(out, m.UpstreamKey)
		}
	}
	return out
}

func (p *Pool) ModelIDs() []string {
	if p == nil {
		return nil
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	seen := map[string]struct{}{}
	var out []string
	for _, m := range p.members {
		for _, model := range m.Models {
			if _, ok := seen[model]; ok {
				continue
			}
			seen[model] = struct{}{}
			out = append(out, model)
		}
	}
	sort.Strings(out)
	return out
}

func (p *Pool) HasModel(model string) bool {
	for _, item := range p.ModelIDs() {
		if item == model {
			return true
		}
	}
	return false
}

func (p *Pool) Snapshot(now time.Time) PoolSnapshot {
	if p == nil {
		return PoolSnapshot{}
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	healthy := 0
	for _, m := range p.members {
		if m.servesAny(now) {
			healthy++
		}
	}
	return PoolSnapshot{
		SchedulePolicy:     p.policy,
		MemberCount:        len(p.members),
		HealthyMemberCount: healthy,
	}
}

func (m *PoolMember) servesAny(now time.Time) bool {
	if !m.healthy || now.Before(m.cooldownUntil) {
		return false
	}
	return len(m.Models) > 0
}

func (p *Pool) Pick(model string, excluded []string, now time.Time) *PoolMember {
	if p == nil {
		return nil
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	var eligible []*PoolMember
	for _, m := range p.members {
		if isExcludedID(m.ID, excluded) {
			continue
		}
		if !m.serves(model, now) {
			continue
		}
		eligible = append(eligible, m)
	}
	if len(eligible) == 0 {
		return nil
	}
	useQuota := true
	for _, m := range eligible {
		if m.QuotaRemainingPct == nil {
			useQuota = false
			break
		}
	}
	sort.SliceStable(eligible, func(i, j int) bool {
		return cmpMember(eligible[i], eligible[j], useQuota) < 0
	})
	var picked *PoolMember
	if p.policy == policyRoundRobin {
		picked = p.pickRoundRobin(eligible)
	} else {
		picked = eligible[0]
	}
	if picked == nil {
		return nil
	}
	out := picked.clone()
	return &out
}

func (p *Pool) MemberByIdentity(memberID, sourceKind, sourceID, refreshKind string) *PoolMember {
	if p == nil {
		return nil
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, member := range p.members {
		if member.ID == memberID && member.SourceKind == sourceKind && member.SourceID == sourceID && member.RefreshKind == refreshKind {
			out := member.clone()
			return &out
		}
	}
	return nil
}

func (p *Pool) pickRoundRobin(eligible []*PoolMember) *PoolMember {
	lead := eligible[0]
	var group []*PoolMember
	for _, m := range eligible {
		if m.Priority == lead.Priority {
			group = append(group, m)
		}
	}
	n := len(group)
	if n == 0 {
		return nil
	}
	key := itoa(int(lead.Priority))
	start := p.rrCursors[key] % n
	p.rrCursors[key] = (start + 1) % n
	return group[start]
}

func cmpMember(left, right *PoolMember, useQuota bool) int {
	if left.Priority != right.Priority {
		if left.Priority < right.Priority {
			return -1
		}
		return 1
	}
	if useQuota && left.QuotaRemainingPct != nil && right.QuotaRemainingPct != nil {
		if *left.QuotaRemainingPct > *right.QuotaRemainingPct {
			return -1
		}
		if *left.QuotaRemainingPct < *right.QuotaRemainingPct {
			return 1
		}
	}
	if left.Position != right.Position {
		if left.Position < right.Position {
			return -1
		}
		return 1
	}
	if left.ID < right.ID {
		return -1
	}
	if left.ID > right.ID {
		return 1
	}
	return 0
}

func isExcludedID(id string, excluded []string) bool {
	for _, item := range excluded {
		if item == id {
			return true
		}
	}
	return false
}

func (p *Pool) ReportSuccess(id string) {
	if p == nil {
		return
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, m := range p.members {
		if m.ID == id {
			m.healthy = true
			return
		}
	}
}

func (p *Pool) ReportFailure(id, model, class string, retryAfter time.Duration, now time.Time) {
	if p == nil {
		return
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, m := range p.members {
		if m.ID != id {
			continue
		}
		cool := cooldownForClass(class, retryAfter)
		switch class {
		case classQuota:
			if model != "" {
				m.modelCool[model] = now.Add(cool)
			} else {
				m.cooldownUntil = now.Add(cool)
			}
		case classAuth:
			m.healthy = false
			m.cooldownUntil = now.Add(cool)
		default:
			if cool > 0 {
				m.cooldownUntil = now.Add(cool)
			}
		}
		return
	}
}

func cooldownForClass(class string, retryAfter time.Duration) time.Duration {
	if retryAfter > 0 {
		if retryAfter > maxCooldown {
			return maxCooldown
		}
		return retryAfter
	}
	switch class {
	case classQuota:
		return quotaCooldown
	case classAuth, classTransient, classEntitlement:
		return defaultCooldown
	default:
		return 0
	}
}

func classifyHTTP(status int) string {
	switch {
	case status == 429:
		return classQuota
	case status == 401:
		return classAuth
	case status == 403 || status == 404:
		return classEntitlement
	case status >= 500 || status == 0:
		return classTransient
	default:
		return classRequest
	}
}

func parseRetryAfter(raw string) time.Duration {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return 0
	}
	var secs int
	for _, c := range raw {
		if c < '0' || c > '9' {
			return 0
		}
		secs = secs*10 + int(c-'0')
	}
	if secs <= 0 {
		return 0
	}
	d := time.Duration(secs) * time.Second
	if d > maxCooldown {
		return maxCooldown
	}
	return d
}

func shouldFailover(class string, committed bool) bool {
	if committed {
		return false
	}
	switch class {
	case classQuota, classAuth, classEntitlement, classTransient:
		return true
	default:
		return false
	}
}
