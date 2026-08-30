import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('SourcePreview chrome', () => {
  it('clips CodeMirror to rounded-card; scroll lives on the inner scroller', () => {
    const src = source('SourcePreview.tsx');
    expect(src).toContain('overflow-hidden rounded-card');
    expect(src).toContain('overflow-auto');
    expect(src).not.toMatch(/className="[^"]*overflow-auto rounded-card/);
  });

  it('uses design-token highlighting and fold, not the VS Code CodeMirror theme', () => {
    const preview = source('SourcePreview.tsx');
    const theme = source('source-preview-theme.ts');
    expect(preview).toContain('theme="none"');
    expect(preview).toContain('foldGutter: foldable');
    expect(theme).toContain('var(--accent)');
    expect(theme).toContain('var(--text-secondary)');
    expect(theme).toContain('var(--info)');
    expect(theme).not.toContain('sk-');
  });

  it('is the JSON/TOML preview for login files, backups, MCP details, and the supplier editor', () => {
    expect(source('ConfigFileCard.tsx')).toContain('<SourcePreview');
    expect(source('ConfigEditor.tsx')).toContain('<SourcePreview');
    expect(
      readFileSync(path.join(dir, '../../pages/mcp/McpServerTable.tsx'), 'utf8'),
    ).toContain('<SourcePreview');
    expect(
      readFileSync(path.join(dir, '../../pages/chat/ChatProcessPanel.tsx'), 'utf8'),
    ).toContain('<SourcePreview');
  });

  it('does not redact or unmask in the preview layer', () => {
    const preview = source('SourcePreview.tsx');
    const helpers = readFileSync(
      path.join(dir, '../../lib/source-preview.ts'),
      'utf8',
    );
    expect(preview).toContain('Does not redact');
    expect(helpers).toContain('never redacts');
    expect(helpers).not.toContain('maskConfigSecrets');
    expect(helpers).not.toContain('redactValue');
  });
});
