import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

function walkSourceFiles(root: string, out: string[] = []): string[] {
  for (const name of readdirSync(root)) {
    if (name === 'node_modules') continue;
    const full = path.join(root, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      walkSourceFiles(full, out);
      continue;
    }
    if (/\.(ts|tsx)$/.test(name) && !/\.(test|spec)\.(ts|tsx)$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

const PAGE_CHROME_REFRESH = [
  'pages/mcp/index.tsx',
  'pages/plugins/index.tsx',
  'pages/projects/index.tsx',
  'pages/routes/board/index.tsx',
  'pages/routes/pool/index.tsx',
  'pages/routes/tokens/index.tsx',
  'pages/routes/activity/index.tsx',
] as const;

describe('PageRefreshButton', () => {
  it('locks secondary + RefreshCw spin, never Loader2', () => {
    const src = source('components/shared/PageRefreshButton.tsx');
    expect(src).toContain('variant="secondary"');
    expect(src).toContain('size="sm"');
    expect(src).toContain('RefreshCw');
    expect(src).toContain('animate-spin');
    expect(src).not.toContain('Loader2');
    expect(src).not.toContain('variant="outline"');
    expect(src).not.toContain('variant="ghost"');
  });

  it('is the page-chrome list refresh on MCP, plugins, projects, and routes', () => {
    for (const rel of PAGE_CHROME_REFRESH) {
      const text = source(rel);
      expect(text, rel).toContain('<PageRefreshButton');
    }
  });

  it('does not leave a second page-chrome refresh dialect on those pages', () => {
    const files = walkSourceFiles(path.join(srcRoot, 'pages'));
    const offenders: string[] = [];
    for (const file of files) {
      const rel = path.relative(srcRoot, file).replaceAll('\\', '/');
      if (!(PAGE_CHROME_REFRESH as readonly string[]).includes(rel)) continue;
      const text = readFileSync(file, 'utf8');
      if (text.includes("t('mcp.page.refresh')") && !text.includes('<PageRefreshButton')) {
        offenders.push(rel);
      }
      if (text.includes("t('plugins.page.refresh')") && !text.includes('<PageRefreshButton')) {
        offenders.push(rel);
      }
      if (text.includes("t('projects.page.refresh')") && !text.includes('<PageRefreshButton')) {
        offenders.push(rel);
      }
      if (text.includes("t('routes.board.refresh')") && !text.includes('<PageRefreshButton')) {
        offenders.push(rel);
      }
    }
    expect(offenders).toEqual([]);
  });
});
