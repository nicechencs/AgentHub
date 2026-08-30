import { describe, expect, it } from 'vitest';
import {
  clipPreviewText,
  formatJsonPayload,
  inferSourceFormat,
  prepareSourcePreview,
  tryPrettyJson,
} from './source-preview';

describe('source preview helpers', () => {
  it('pretty-prints objects and arrays without touching already-masked secrets', () => {
    const raw = '{"api_key":"***","env":{"OPENAI_API_KEY":"***","safe":"ok"}}';
    const pretty = tryPrettyJson(raw);
    expect(pretty).toContain('\n');
    expect(pretty).toContain('"api_key": "***"');
    expect(pretty).toContain('"OPENAI_API_KEY": "***"');
    expect(pretty).toContain('"safe": "ok"');
    expect(pretty).not.toMatch(/sk-|xai-/);
  });

  it('does not invent JSON from plain text or primitives', () => {
    expect(tryPrettyJson('not json')).toBeNull();
    expect(tryPrettyJson('"just a string"')).toBeNull();
    expect(tryPrettyJson('{"trailing": true,}')).toBeNull();
  });

  it('infers format from hint, filename, then body', () => {
    expect(inferSourceFormat({ text: 'x = 1', hint: 'toml' })).toBe('toml');
    expect(inferSourceFormat({ text: 'x', fileName: 'settings.json' })).toBe('json');
    expect(inferSourceFormat({ text: 'x', fileName: 'config.toml' })).toBe('toml');
    expect(inferSourceFormat({ text: '{"a":1}' })).toBe('json');
    expect(inferSourceFormat({ text: 'export FOO=1', fileName: '.credentials.yaml' })).toBe(
      'text',
    );
  });

  it('pretty-prepares JSON for read-only display and clips huge payloads', () => {
    expect(prepareSourcePreview('{"a":1}', 'json')).toBe('{\n  "a": 1\n}');
    expect(prepareSourcePreview('model = "x"', 'toml')).toBe('model = "x"');
    const huge = `{"k":"${'x'.repeat(20_000)}"}`;
    const clipped = prepareSourcePreview(huge, 'json');
    expect(clipped.endsWith('\n…')).toBe(true);
    expect(clipped.length).toBeLessThan(huge.length);
  });

  it('formats session payloads without masking; leaves non-JSON strings alone', () => {
    expect(formatJsonPayload({ path: '/tmp', secret: 'sk-live-not-masked' })).toContain(
      'sk-live-not-masked',
    );
    expect(formatJsonPayload('{"a":1}')).toBe('{\n  "a": 1\n}');
    expect(formatJsonPayload('plain tool text')).toBe('plain tool text');
    expect(formatJsonPayload(null)).toBeNull();
  });

  it('clips at the requested bound', () => {
    expect(clipPreviewText('abcd', 4)).toBe('abcd');
    expect(clipPreviewText('abcde', 4)).toBe('abcd\n…');
  });
});
