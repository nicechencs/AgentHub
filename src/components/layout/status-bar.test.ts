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
    const bar = source('components/layout/StatusBar.tsx');
    expect(bar).toContain('AgentStatusStrip');
    expect(bar).toContain('chrome.localForward');
    expect(bar).toContain('chrome.localForwardOpenBoard');
    expect(bar).toContain('ROUTES_BOARD_PATH');
    expect(bar).toContain('<Hint label={aria}>');
    expect(bar).not.toContain('title={aria}');
    expect(bar.indexOf('navigate(ROUTES_BOARD_PATH)')).toBeLessThan(bar.indexOf('<AgentStatusStrip'));
    const strip = source('components/layout/AgentStatusStrip.tsx');
    expect(strip).toContain('title={fractionLabel}');
    expect(strip).toContain('nav.agentsInstalled');
  });
});
