package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"strings"
)

var errInvalidOfficialCodexRequest = errors.New("official Codex request is invalid")

var officialCodexResponseKeys = map[string]struct{}{
	"model":        {},
	"input":        {},
	"stream":       {},
	"store":        {},
	"instructions": {},
	"tools":        {},
	"tool_choice":  {},
	"top_p":        {},
}

// prepareOfficialCodexRequest reshapes a downstream Responses object for the
// official ChatGPT Codex upstream. configuredModel is optional; a usable
// configured model wins over the request model. The returned bool is the
// downstream stream preference captured before the upstream request is forced
// to stream.
func prepareOfficialCodexRequest(raw []byte, configuredModel string) ([]byte, bool, error) {
	body, err := decodeOfficialCodexRequest(raw)
	if err != nil {
		return nil, false, err
	}
	downstreamStream, _ := body["stream"].(bool)

	applyOfficialCodexModel(body, configuredModel)
	body["store"] = false
	body["stream"] = true
	foldOfficialCodexSystemItems(body)
	for key := range body {
		if _, allowed := officialCodexResponseKeys[key]; !allowed {
			delete(body, key)
		}
	}

	prepared, err := json.Marshal(body)
	if err != nil {
		return nil, false, errInvalidOfficialCodexRequest
	}
	return prepared, downstreamStream, nil
}

func decodeOfficialCodexRequest(raw []byte) (map[string]any, error) {
	decoder := json.NewDecoder(bytes.NewReader(raw))
	decoder.UseNumber()
	var body map[string]any
	if err := decoder.Decode(&body); err != nil || body == nil {
		return nil, errInvalidOfficialCodexRequest
	}
	var trailing any
	if err := decoder.Decode(&trailing); err != io.EOF {
		return nil, errInvalidOfficialCodexRequest
	}
	return body, nil
}

func applyOfficialCodexModel(body map[string]any, configuredModel string) {
	configuredModel = strings.TrimSpace(configuredModel)
	if configuredModel != "" && !isLeftoverBridgeModel(configuredModel) {
		body["model"] = configuredModel
		return
	}
	incoming, _ := body["model"].(string)
	incoming = strings.TrimSpace(incoming)
	if incoming == "" || isLeftoverBridgeModel(incoming) {
		delete(body, "model")
		return
	}
	body["model"] = incoming
}

func isLeftoverBridgeModel(model string) bool {
	model = strings.TrimSpace(model)
	return strings.HasPrefix(model, "grok-") ||
		strings.HasPrefix(model, "claude-") ||
		strings.HasPrefix(model, "kimi-") ||
		strings.HasPrefix(model, "deepseek-") ||
		(strings.HasPrefix(model, "agenthub_") && strings.HasSuffix(model, "_bridge"))
}

func foldOfficialCodexSystemItems(body map[string]any) {
	input, ok := body["input"].([]any)
	if !ok {
		return
	}
	folded := make([]string, 0, 2)
	kept := make([]any, 0, len(input))
	for _, rawItem := range input {
		item, ok := rawItem.(map[string]any)
		if !ok {
			kept = append(kept, rawItem)
			continue
		}
		role, _ := item["role"].(string)
		if role != "system" && role != "developer" {
			kept = append(kept, rawItem)
			continue
		}
		if text := officialCodexItemText(item); text != "" {
			folded = append(folded, text)
		}
	}
	body["input"] = kept
	foldedText := strings.Join(folded, "\n")
	if foldedText == "" {
		return
	}

	existing, _ := body["instructions"].(string)
	if existing != "" {
		body["instructions"] = mergeOfficialCodexText(existing, foldedText)
		return
	}
	if prependOfficialCodexTextToFirstUser(kept, foldedText) {
		return
	}
	body["instructions"] = foldedText
}

func officialCodexItemText(item map[string]any) string {
	switch content := item["content"].(type) {
	case string:
		return content
	case []any:
		var text strings.Builder
		for _, rawPart := range content {
			part, ok := rawPart.(map[string]any)
			if !ok {
				continue
			}
			value, _ := part["text"].(string)
			if value != "" {
				text.WriteString(value)
			}
		}
		return text.String()
	default:
		return ""
	}
}

func prependOfficialCodexTextToFirstUser(input []any, foldedText string) bool {
	for _, rawItem := range input {
		item, ok := rawItem.(map[string]any)
		if !ok || item["role"] != "user" {
			continue
		}
		switch content := item["content"].(type) {
		case string:
			item["content"] = mergeOfficialCodexText(foldedText, content)
		case []any:
			for _, rawPart := range content {
				part, ok := rawPart.(map[string]any)
				if !ok {
					continue
				}
				text, ok := part["text"].(string)
				if !ok {
					continue
				}
				part["text"] = mergeOfficialCodexText(foldedText, text)
				return true
			}
			item["content"] = append([]any{map[string]any{
				"type": "input_text",
				"text": foldedText,
			}}, content...)
		default:
			item["content"] = []any{map[string]any{
				"type": "input_text",
				"text": foldedText,
			}}
		}
		return true
	}
	return false
}

func mergeOfficialCodexText(first, second string) string {
	if first == "" {
		return second
	}
	if second == "" {
		return first
	}
	return first + "\n" + second
}
