package main

import (
	"bytes"
	"crypto/rand"
	"crypto/sha1"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"net/http"
	"strings"
	"sync"
	"unicode/utf8"
)

const (
	grokOfficialCLIVersion      = "1.0.44"
	grokOfficialTokenAuth       = "xai-grok-cli"
	grokOfficialClientID        = "grok-shell"
	grokOfficialClientMode      = "headless"
	grokOfficialMaxSeedBytes    = 1024
	grokOfficialMaxTitleBytes   = 4096
	grokOfficialReplayEntries   = 64
	grokOfficialReplayBodyBytes = 8 << 20
)

var (
	grokOfficialAgentIDOnce sync.Once
	grokOfficialAgentID     string
	urlNamespaceUUID        = [16]byte{0x6b, 0xa7, 0xb8, 0x11, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8}
)

type grokOfficialRequestIdentity struct {
	RequestID     string
	SessionID     string
	ModelOverride string
}

type grokOfficialPreparedRequest struct {
	Body      map[string]any
	Identity  grokOfficialRequestIdentity
	CacheSeed string
	SourceID  string
	Model     string
}

func prepareGrokOfficialRequest(raw []byte, inbound http.Header, sourceID, requestID, modelOverride string) (*grokOfficialPreparedRequest, error) {
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	var body map[string]any
	if err := decoder.Decode(&body); err != nil || body == nil {
		return nil, errors.New("Grok request body is not a JSON object")
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return nil, errors.New("Grok request body has trailing data")
	}

	seed := extractGrokOfficialPromptCacheSeed(inbound, body)
	modelOverride = strings.TrimSpace(modelOverride)
	if modelOverride != "" {
		body["model"] = modelOverride
	}
	normalizeGrokOfficialTools(body)
	sanitizeGrokOfficialRequest(body)
	injectGrokOfficialPromptCacheKey(body, seed)

	model := modelOverride
	if model == "" {
		model, _ = body["model"].(string)
		model = strings.TrimSpace(model)
	}
	return &grokOfficialPreparedRequest{
		Body:      body,
		Identity:  newGrokOfficialRequestIdentity(requestID, seed, sourceID, model),
		CacheSeed: seed,
		SourceID:  strings.TrimSpace(sourceID),
		Model:     model,
	}, nil
}

func (prepared *grokOfficialPreparedRequest) marshalBody() ([]byte, error) {
	if prepared == nil || prepared.Body == nil {
		return nil, errors.New("Grok request is missing")
	}
	return json.Marshal(prepared.Body)
}

func newGrokOfficialRequestIdentity(requestID, seed, sourceID, modelOverride string) grokOfficialRequestIdentity {
	requestID = strings.TrimSpace(requestID)
	if requestID == "" {
		requestID = newGrokOfficialUUIDv4()
	}
	return grokOfficialRequestIdentity{
		RequestID:     requestID,
		SessionID:     grokOfficialSessionID(seed, sourceID),
		ModelOverride: strings.TrimSpace(modelOverride),
	}
}

// applyGrokOfficialHeaders starts from an explicit allowlist. Inbound authorization,
// cookie, proxy and user-agent values must never reach cli-chat-proxy.
func applyGrokOfficialHeaders(req *http.Request, bearer string, identity grokOfficialRequestIdentity) error {
	if req == nil || !validGrokOfficialHeaderValue(bearer) || strings.TrimSpace(bearer) == "" ||
		strings.TrimSpace(identity.RequestID) == "" ||
		!validGrokOfficialHeaderValue(identity.RequestID) || !validGrokOfficialHeaderValue(identity.SessionID) ||
		!validGrokOfficialHeaderValue(identity.ModelOverride) {
		return errors.New("Grok request identity is invalid")
	}
	agentID := stableGrokOfficialAgentID()
	traceparent := newGrokOfficialTraceparent()
	if agentID == "" || traceparent == "" {
		return errors.New("Grok request identity is unavailable")
	}
	for _, name := range []string{
		"Authorization", "Cookie", "Proxy-Authorization", "User-Agent",
		"X-Xai-Token-Auth", "X-Grok-Client-Version", "X-Grok-Client-Identifier",
		"X-Grok-Client-Mode", "X-AuthenticateResponse", "X-Grok-Agent-Id",
		"X-Grok-Req-Id", "X-Grok-Session-Id", "X-Grok-Conv-Id",
		"X-Grok-Model-Override", "Traceparent",
	} {
		req.Header.Del(name)
	}
	req.Header.Set("Authorization", "Bearer "+bearer)
	req.Header.Set("X-Xai-Token-Auth", grokOfficialTokenAuth)
	req.Header.Set("X-Grok-Client-Version", grokOfficialCLIVersion)
	req.Header.Set("X-Grok-Client-Identifier", grokOfficialClientID)
	req.Header.Set("X-Grok-Client-Mode", grokOfficialClientMode)
	req.Header.Set("X-AuthenticateResponse", "authenticate-response")
	req.Header.Set("User-Agent", "grok-pager/"+grokOfficialCLIVersion+" grok-shell/"+grokOfficialCLIVersion)
	req.Header.Set("X-Grok-Agent-Id", agentID)
	req.Header.Set("X-Grok-Req-Id", identity.RequestID)
	if identity.SessionID != "" {
		req.Header.Set("X-Grok-Session-Id", identity.SessionID)
		req.Header.Set("X-Grok-Conv-Id", identity.SessionID)
	}
	if identity.ModelOverride != "" {
		req.Header.Set("X-Grok-Model-Override", identity.ModelOverride)
	}
	req.Header.Set("Traceparent", traceparent)
	return nil
}

func validGrokOfficialHeaderValue(value string) bool {
	for i := 0; i < len(value); i++ {
		if value[i] == '\t' {
			continue
		}
		if value[i] < 0x20 || value[i] == 0x7f {
			return false
		}
	}
	return true
}

func stableGrokOfficialAgentID() string {
	grokOfficialAgentIDOnce.Do(func() { grokOfficialAgentID = newGrokOfficialUUIDv4() })
	return grokOfficialAgentID
}

func newGrokOfficialUUIDv4() string {
	var value [16]byte
	if _, err := rand.Read(value[:]); err != nil {
		return ""
	}
	value[6] = (value[6] & 0x0f) | 0x40
	value[8] = (value[8] & 0x3f) | 0x80
	return formatGrokOfficialUUID(value)
}

func newGrokOfficialTraceparent() string {
	var value [24]byte
	if _, err := rand.Read(value[:]); err != nil {
		return ""
	}
	if allZero(value[:16]) {
		value[0] = 1
	}
	if allZero(value[16:]) {
		value[16] = 1
	}
	return "00-" + hex.EncodeToString(value[:16]) + "-" + hex.EncodeToString(value[16:]) + "-01"
}

func allZero(value []byte) bool {
	for _, item := range value {
		if item != 0 {
			return false
		}
	}
	return true
}

func grokOfficialSessionID(seed, sourceID string) string {
	seed = strings.TrimSpace(seed)
	if seed == "" {
		return ""
	}
	sourceID = strings.TrimSpace(sourceID)
	if sourceID == "" {
		if parsed, ok := parseGrokOfficialUUID(seed); ok {
			return formatGrokOfficialUUID(parsed)
		}
		return grokOfficialUUIDv5("agenthub:grok-session:" + seed)
	}
	return grokOfficialUUIDv5("agenthub:grok-session:" + sourceID + ":" + seed)
}

func grokOfficialUUIDv5(name string) string {
	hasher := sha1.New()
	_, _ = hasher.Write(urlNamespaceUUID[:])
	_, _ = hasher.Write([]byte(name))
	sum := hasher.Sum(nil)
	var value [16]byte
	copy(value[:], sum[:16])
	value[6] = (value[6] & 0x0f) | 0x50
	value[8] = (value[8] & 0x3f) | 0x80
	return formatGrokOfficialUUID(value)
}

func parseGrokOfficialUUID(raw string) ([16]byte, bool) {
	var out [16]byte
	compact := strings.ReplaceAll(strings.TrimSpace(raw), "-", "")
	if len(compact) != 32 {
		return out, false
	}
	decoded, err := hex.DecodeString(compact)
	if err != nil || len(decoded) != len(out) {
		return out, false
	}
	copy(out[:], decoded)
	return out, true
}

func formatGrokOfficialUUID(value [16]byte) string {
	encoded := hex.EncodeToString(value[:])
	return encoded[:8] + "-" + encoded[8:12] + "-" + encoded[12:16] + "-" + encoded[16:20] + "-" + encoded[20:]
}

func extractGrokOfficialPromptCacheSeed(headers http.Header, body map[string]any) string {
	claudeSession := normalizedGrokOfficialSeed(headers.Get("X-Claude-Code-Session-Id"))
	if claudeSession != "" && isGrokOfficialClaudeTitleRequest(body) {
		return ""
	}
	if claudeSession != "" {
		agent := normalizedGrokOfficialSeed(headers.Get("X-Claude-Code-Agent-Id"))
		if agent == "" {
			agent = "main"
		}
		return normalizedGrokOfficialSeed("claude:" + claudeSession + ":agent:" + agent)
	}
	if seed := grokOfficialSeedFromCodexMetadata(headers.Get("X-Codex-Turn-Metadata")); seed != "" {
		return seed
	}
	if windowID := normalizedGrokOfficialSeed(headers.Get("X-Codex-Window-Id")); windowID != "" {
		return normalizedGrokOfficialSeed("codex:window:" + windowID)
	}
	for _, name := range []string{"X-Session-Id", "Session-Id", "X-Conversation-Id", "X-Client-Session-Id", "X-Grok-Conv-Id"} {
		if seed := normalizedGrokOfficialSeed(headers.Get(name)); seed != "" {
			return seed
		}
	}
	if seed := normalizedGrokOfficialSeed(grokOfficialString(body["prompt_cache_key"])); seed != "" {
		return seed
	}
	if metadata, ok := body["metadata"].(map[string]any); ok {
		for _, key := range []string{"session_id", "sessionId"} {
			if seed := normalizedGrokOfficialSeed(grokOfficialString(metadata[key])); seed != "" {
				return seed
			}
		}
	}
	if clientMetadata, ok := body["client_metadata"].(map[string]any); ok {
		if seed := grokOfficialSeedFromCodexMetadataValue(clientMetadata["x-codex-turn-metadata"]); seed != "" {
			return seed
		}
	}
	for _, key := range []string{"session_id", "sessionId", "conversation_id", "conversationId"} {
		if seed := normalizedGrokOfficialSeed(grokOfficialString(body[key])); seed != "" {
			return seed
		}
	}
	return ""
}

func normalizedGrokOfficialSeed(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}
	if len(raw) <= grokOfficialMaxSeedBytes {
		return raw
	}
	end := grokOfficialMaxSeedBytes
	for end > 0 && !utf8.RuneStart(raw[end]) {
		end--
	}
	return raw[:end]
}

func grokOfficialSeedFromCodexMetadata(raw string) string {
	if strings.TrimSpace(raw) == "" {
		return ""
	}
	var value map[string]any
	if json.Unmarshal([]byte(raw), &value) != nil {
		return ""
	}
	return grokOfficialSeedFromCodexMetadataValue(value)
}

func grokOfficialSeedFromCodexMetadataValue(value any) string {
	if raw, ok := value.(string); ok {
		return grokOfficialSeedFromCodexMetadata(raw)
	}
	metadata, ok := value.(map[string]any)
	if !ok {
		return ""
	}
	if seed := normalizedGrokOfficialSeed(grokOfficialString(metadata["prompt_cache_key"])); seed != "" {
		return seed
	}
	if windowID := normalizedGrokOfficialSeed(grokOfficialString(metadata["window_id"])); windowID != "" {
		return normalizedGrokOfficialSeed("codex:window:" + windowID)
	}
	return ""
}

func isGrokOfficialClaudeTitleRequest(body map[string]any) bool {
	var builder strings.Builder
	appendGrokOfficialRoleText(&builder, body["messages"], map[string]bool{"user": true})
	appendGrokOfficialRoleText(&builder, body["input"], map[string]bool{"user": true})
	appendGrokOfficialText(&builder, body["system"])
	appendGrokOfficialText(&builder, body["instructions"])
	appendGrokOfficialRoleText(&builder, body["messages"], map[string]bool{"system": true})
	appendGrokOfficialRoleText(&builder, body["input"], map[string]bool{"system": true, "developer": true})
	text := strings.ToLower(builder.String())
	return strings.Contains(text, "generate a concise") && strings.Contains(text, "title") && strings.Contains(text, "coding session")
}

func appendGrokOfficialRoleText(builder *strings.Builder, value any, roles map[string]bool) {
	items, ok := value.([]any)
	if !ok || builder.Len() >= grokOfficialMaxTitleBytes {
		return
	}
	for _, raw := range items {
		item, ok := raw.(map[string]any)
		if !ok || !roles[strings.ToLower(grokOfficialString(item["role"]))] {
			continue
		}
		appendGrokOfficialText(builder, item["content"])
		appendGrokOfficialText(builder, item["text"])
		if builder.Len() >= grokOfficialMaxTitleBytes {
			return
		}
	}
}

func appendGrokOfficialText(builder *strings.Builder, value any) {
	if builder.Len() >= grokOfficialMaxTitleBytes {
		return
	}
	switch typed := value.(type) {
	case string:
		appendGrokOfficialChunk(builder, typed)
	case []any:
		for _, item := range typed {
			appendGrokOfficialText(builder, item)
			if builder.Len() >= grokOfficialMaxTitleBytes {
				return
			}
		}
	case map[string]any:
		appendGrokOfficialText(builder, typed["text"])
		appendGrokOfficialText(builder, typed["content"])
	}
}

func appendGrokOfficialChunk(builder *strings.Builder, text string) {
	if text == "" || builder.Len() >= grokOfficialMaxTitleBytes {
		return
	}
	if builder.Len() > 0 {
		builder.WriteByte(' ')
	}
	remaining := grokOfficialMaxTitleBytes - builder.Len()
	if len(text) > remaining {
		end := remaining
		for end > 0 && !utf8.RuneStart(text[end]) {
			end--
		}
		text = text[:end]
	}
	builder.WriteString(text)
}

func normalizeGrokOfficialTools(body map[string]any) {
	tools, ok := body["tools"].([]any)
	if !ok {
		return
	}
	declaredShell := false
	for _, raw := range tools {
		if tool, ok := raw.(map[string]any); ok && grokOfficialString(tool["type"]) == "shell" {
			declaredShell = true
		}
	}
	out := make([]any, 0, len(tools))
	for _, raw := range tools {
		tool, ok := raw.(map[string]any)
		if !ok {
			out = append(out, raw)
			continue
		}
		switch grokOfficialString(tool["type"]) {
		case "local_shell":
			if !declaredShell {
				out = append(out, map[string]any{"type": "shell", "environment": map[string]any{"type": "local"}})
				declaredShell = true
			}
		case "apply_patch":
			out = append(out, grokOfficialApplyPatchTool())
		default:
			out = append(out, raw)
		}
	}
	body["tools"] = out
}

func grokOfficialApplyPatchTool() map[string]any {
	return map[string]any{
		"type": "function", "name": "apply_patch", "strict": true,
		"description": "Apply a file change. operation.type is one of create_file, update_file, or delete_file. operation.path is the target path. operation.diff is the patch text for create_file and update_file; use an empty string for delete_file.",
		"parameters": map[string]any{
			"type": "object", "required": []any{"operation"}, "additionalProperties": false,
			"properties": map[string]any{"operation": map[string]any{
				"type": "object", "required": []any{"type", "path", "diff"}, "additionalProperties": false,
				"properties": map[string]any{
					"type": map[string]any{"type": "string", "enum": []any{"create_file", "update_file", "delete_file"}},
					"path": map[string]any{"type": "string", "minLength": json.Number("1")},
					"diff": map[string]any{"type": "string"},
				},
			}},
		},
	}
}

var grokOfficialToolTypes = map[string]bool{
	"function": true, "web_search": true, "x_search": true, "image_generation": true,
	"file_search": true, "code_interpreter": true, "mcp": true, "shell": true, "tool_search": true,
}

var grokOfficialReasoningSummaries = map[string]bool{"auto": true, "concise": true, "detailed": true}

func sanitizeGrokOfficialRequest(body map[string]any) {
	delete(body, "client_metadata")
	if value, exists := body["store"]; exists {
		if _, ok := value.(bool); !ok {
			delete(body, "store")
		}
	}
	sanitizeGrokOfficialReasoning(body)
	keptNames, toolsWereArray := sanitizeGrokOfficialToolList(body)
	sanitizeGrokOfficialToolChoice(body, keptNames, toolsWereArray)
	sanitizeGrokOfficialReasoningInputs(body)
}

func sanitizeGrokOfficialReasoning(body map[string]any) {
	value, exists := body["reasoning"]
	if !exists {
		return
	}
	reasoning, ok := value.(map[string]any)
	if !ok {
		delete(body, "reasoning")
		return
	}
	cleaned := make(map[string]any)
	for _, key := range []string{"effort", "summary", "generate_summary"} {
		item, exists := reasoning[key]
		if !exists {
			continue
		}
		if item == nil {
			cleaned[key] = nil
			continue
		}
		text, ok := item.(string)
		if !ok {
			continue
		}
		if key == "effort" || grokOfficialReasoningSummaries[text] {
			cleaned[key] = text
		}
	}
	if len(cleaned) == 0 {
		delete(body, "reasoning")
	} else {
		body["reasoning"] = cleaned
	}
}

func sanitizeGrokOfficialToolList(body map[string]any) ([]string, bool) {
	tools, ok := body["tools"].([]any)
	if !ok {
		return nil, false
	}
	cleaned := make([]any, 0, len(tools))
	keptNames := make([]string, 0, len(tools))
	for _, raw := range tools {
		tool, ok := raw.(map[string]any)
		if !ok || !grokOfficialToolTypes[grokOfficialString(tool["type"])] {
			continue
		}
		if grokOfficialString(tool["type"]) == "web_search" {
			delete(tool, "external_web_access")
			delete(tool, "search_context_size")
			delete(tool, "user_location")
		}
		if name := grokOfficialString(tool["name"]); name != "" {
			keptNames = append(keptNames, name)
		}
		cleaned = append(cleaned, tool)
	}
	if len(cleaned) == 0 {
		delete(body, "tools")
	} else {
		body["tools"] = cleaned
	}
	return keptNames, true
}

func sanitizeGrokOfficialToolChoice(body map[string]any, keptNames []string, toolsWereArray bool) {
	tools, toolsPresent := body["tools"].([]any)
	if !toolsPresent || len(tools) == 0 {
		if toolsWereArray {
			delete(body, "tool_choice")
			delete(body, "parallel_tool_calls")
		}
		return
	}
	choice, exists := body["tool_choice"]
	if !exists {
		return
	}
	drop := false
	switch typed := choice.(type) {
	case string:
		drop = typed != "auto" && typed != "none" && typed != "required"
	case map[string]any:
		name := grokOfficialString(typed["name"])
		if name == "" {
			if function, ok := typed["function"].(map[string]any); ok {
				name = grokOfficialString(function["name"])
			}
		}
		if name != "" {
			drop = true
			for _, kept := range keptNames {
				if kept == name {
					drop = false
					break
				}
			}
		}
	default:
		drop = true
	}
	if drop {
		delete(body, "tool_choice")
	}
}

func sanitizeGrokOfficialReasoningInputs(body map[string]any) {
	input, ok := body["input"].([]any)
	if !ok {
		return
	}
	for _, raw := range input {
		item, ok := raw.(map[string]any)
		if !ok || grokOfficialString(item["type"]) != "reasoning" {
			continue
		}
		if content, exists := item["content"]; exists {
			if _, ok := content.([]any); !ok {
				delete(item, "content")
			}
		}
	}
}

func injectGrokOfficialPromptCacheKey(body map[string]any, seed string) {
	seed = strings.TrimSpace(seed)
	if seed == "" || strings.TrimSpace(grokOfficialString(body["prompt_cache_key"])) != "" {
		return
	}
	body["prompt_cache_key"] = seed
}

func grokOfficialString(value any) string {
	text, _ := value.(string)
	return text
}

type grokOfficialReasoningReplay struct {
	mu      sync.Mutex
	entries map[string][]map[string]any
	order   []string
}

func newGrokOfficialReasoningReplay() *grokOfficialReasoningReplay {
	return &grokOfficialReasoningReplay{entries: make(map[string][]map[string]any)}
}

func (replay *grokOfficialReasoningReplay) apply(body map[string]any, sourceID, model, session string) {
	if replay == nil || body == nil || strings.TrimSpace(session) == "" || strings.TrimSpace(model) == "" ||
		strings.TrimSpace(grokOfficialString(body["previous_response_id"])) != "" {
		return
	}
	if input, ok := body["input"].([]any); ok {
		for _, item := range input {
			if isGrokOfficialEncryptedReasoning(item) {
				return
			}
		}
	}
	key := grokOfficialReplayKey(sourceID, model, session)
	replay.mu.Lock()
	items := cloneGrokOfficialReasoningItems(replay.entries[key])
	replay.mu.Unlock()
	if len(items) == 0 {
		return
	}
	input := grokOfficialInputArray(body["input"])
	merged := make([]any, 0, len(items)+len(input))
	for _, item := range items {
		merged = append(merged, item)
	}
	merged = append(merged, input...)
	body["input"] = merged
}

func (replay *grokOfficialReasoningReplay) storeCompleted(sourceID, model, session string, completed map[string]any) {
	if replay == nil || strings.TrimSpace(session) == "" || strings.TrimSpace(model) == "" || completed == nil {
		return
	}
	encoded, err := json.Marshal(completed)
	if err != nil || len(encoded) > grokOfficialReplayBodyBytes {
		return
	}
	items := extractGrokOfficialReasoningItems(completed)
	key := grokOfficialReplayKey(sourceID, model, session)
	replay.mu.Lock()
	defer replay.mu.Unlock()
	if len(items) == 0 {
		delete(replay.entries, key)
		replay.removeOrderKey(key)
		return
	}
	if _, exists := replay.entries[key]; !exists {
		if len(replay.entries) >= grokOfficialReplayEntries && len(replay.order) > 0 {
			delete(replay.entries, replay.order[0])
			replay.order = replay.order[1:]
		}
		replay.order = append(replay.order, key)
	}
	replay.entries[key] = cloneGrokOfficialReasoningItems(items)
}

func (replay *grokOfficialReasoningReplay) storeSSE(sourceID, model, session string, raw []byte) {
	if replay == nil || len(raw) > grokOfficialReplayBodyBytes {
		return
	}
	var completed map[string]any
	for _, line := range strings.Split(string(raw), "\n") {
		data, ok := strings.CutPrefix(line, "data:")
		if !ok {
			continue
		}
		data = strings.TrimSpace(data)
		if data == "" || data == "[DONE]" {
			continue
		}
		var event map[string]any
		if json.Unmarshal([]byte(data), &event) != nil || grokOfficialString(event["type"]) != "response.completed" {
			continue
		}
		if response, ok := event["response"].(map[string]any); ok {
			completed = response
		} else {
			completed = event
		}
	}
	if completed != nil {
		replay.storeCompleted(sourceID, model, session, completed)
	}
}

func (replay *grokOfficialReasoningReplay) clear(sourceID, model, session string) {
	if replay == nil {
		return
	}
	key := grokOfficialReplayKey(sourceID, model, session)
	replay.mu.Lock()
	delete(replay.entries, key)
	replay.removeOrderKey(key)
	replay.mu.Unlock()
}

// recoverDecodeFailure performs the sole safe replay: clear the cached chain and
// strip encrypted reasoning. The caller owns alreadyRetried and must set it after true.
func (replay *grokOfficialReasoningReplay) recoverDecodeFailure(body map[string]any, sourceID, model, session string, responseBody []byte, alreadyRetried bool) bool {
	if alreadyRetried || !isGrokOfficialReasoningDecodeFailure(responseBody) {
		return false
	}
	replay.clear(sourceID, model, session)
	return stripGrokOfficialEncryptedReasoning(body)
}

func (replay *grokOfficialReasoningReplay) removeOrderKey(key string) {
	for index, existing := range replay.order {
		if existing == key {
			replay.order = append(replay.order[:index], replay.order[index+1:]...)
			return
		}
	}
}

func grokOfficialReplayKey(sourceID, model, session string) string {
	return strings.TrimSpace(sourceID) + "\x00" + strings.TrimSpace(model) + "\x00" + strings.TrimSpace(session)
}

func grokOfficialInputArray(value any) []any {
	switch typed := value.(type) {
	case []any:
		return append([]any(nil), typed...)
	case string:
		return []any{map[string]any{
			"type": "message", "role": "user",
			"content": []any{map[string]any{"type": "input_text", "text": typed}},
		}}
	default:
		return nil
	}
}

func extractGrokOfficialReasoningItems(completed map[string]any) []map[string]any {
	output, ok := completed["output"].([]any)
	if !ok {
		if response, responseOK := completed["response"].(map[string]any); responseOK {
			output, ok = response["output"].([]any)
		}
	}
	if !ok {
		return nil
	}
	items := make([]map[string]any, 0)
	for _, raw := range output {
		if item, ok := raw.(map[string]any); ok && isGrokOfficialEncryptedReasoning(item) {
			items = append(items, item)
		}
	}
	return items
}

func cloneGrokOfficialReasoningItems(items []map[string]any) []map[string]any {
	if len(items) == 0 {
		return nil
	}
	raw, err := json.Marshal(items)
	if err != nil {
		return nil
	}
	var cloned []map[string]any
	if json.Unmarshal(raw, &cloned) != nil {
		return nil
	}
	return cloned
}

func isGrokOfficialEncryptedReasoning(value any) bool {
	item, ok := value.(map[string]any)
	return ok && grokOfficialString(item["type"]) == "reasoning" && strings.TrimSpace(grokOfficialString(item["encrypted_content"])) != ""
}

func stripGrokOfficialEncryptedReasoning(body map[string]any) bool {
	if body == nil {
		return false
	}
	input, ok := body["input"].([]any)
	if !ok {
		return false
	}
	cleaned := make([]any, 0, len(input))
	for _, item := range input {
		if !isGrokOfficialEncryptedReasoning(item) {
			cleaned = append(cleaned, item)
		}
	}
	if len(cleaned) == len(input) {
		return false
	}
	body["input"] = cleaned
	return true
}

func isGrokOfficialReasoningDecodeFailure(body []byte) bool {
	lower := strings.ToLower(string(body))
	return strings.Contains(lower, "could not decode the compaction blob") ||
		strings.Contains(lower, "could not decrypt the provided encrypted_content")
}
