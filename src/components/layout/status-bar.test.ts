import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('status bar wiring', () => {
  it('spans the window under the shell columns', () => {
    const app = source('App.tsx');
    expect(app).toContain('pageRhythm.shellBody');
    expect(app).toContain('<StatusBar />');
    expect(app.indexOf('pageRhythm.shellBody')).toBeLessThan(app.indexOf('<StatusBar />'));
  });

  it('keeps agent dots on the status bar, not the sidebar', () => {
    expect(source('components/layout/Sidebar.tsx')).not.toContain('AgentDot');
    expect(source('components/layout/AgentStatusStrip.tsx')).toContain('AgentDot');
    expect(source('components/layout/StatusBar.tsx')).toContain('AgentStatusStrip');
    expect(source('components/layout/StatusBar.tsx')).toContain('chrome.localForward');
    expect(source('components/layout/StatusBar.tsx')).toContain('ROUTES_BOARD_PATH');
  });
});
