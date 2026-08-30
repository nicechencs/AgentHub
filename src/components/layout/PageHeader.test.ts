import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('PageHeader', () => {
  it('does not draw a rule under the title; Skills/Projects match other pages', () => {
    expect(source('components/layout/PageHeader.tsx')).not.toContain('border-b');
    expect(source('components/layout/SideSplit.tsx')).not.toContain('border-b');
    expect(source('pages/skills/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/projects/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/connections/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/bridges/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/plugins/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/agents/index.tsx')).toContain('WorkbenchSplitPage');
  });

  it('uses the split-list inset when a preview pane is mounted', () => {
    expect(source('components/layout/SideSplit.tsx')).toContain('workbenchXSplit');
    expect(source('components/layout/SideSplit.tsx')).toContain("listOverflowX === 'hidden'");
    expect(source('pages/projects/index.tsx')).toContain('listOverflowX="hidden"');
  });

  it('keeps page commands in the list column, left of the separator', () => {
    const split = source('components/layout/SideSplit.tsx');
    const pageFn = split.slice(split.indexOf('export function WorkbenchSplitPage'));
    const splitRefAt = pageFn.indexOf('split.splitRef');
    const listColAt = pageFn.indexOf('flex min-h-0 min-w-0 flex-1 flex-col');
    const inspectAt = pageFn.indexOf('<SideSplitFrame');
    expect(splitRefAt).toBeGreaterThan(0);
    expect(listColAt).toBeGreaterThan(splitRefAt);
    expect(inspectAt).toBeGreaterThan(listColAt);
    const listFooterAt = pageFn.indexOf('{listFooter ?');
    expect(listFooterAt).toBeGreaterThan(listColAt);
    expect(inspectAt).toBeGreaterThan(listFooterAt);

    expect(source('pages/projects/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/skills/index.tsx')).toContain('pageRhythm.chromeActions');
  });

  it('starts non-Chat body flush under the title slot', () => {
    expect(source('components/layout/PageHeader.tsx')).toContain('return null');
    expect(source('components/layout/page-rhythm.ts')).toContain('workbenchY: pageInsetTw.b');
    expect(source('pages/skills/index.tsx')).toContain('pageRhythm.chromeRow');
    expect(source('pages/skills/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/skills/index.tsx')).not.toContain('className="mb-2"');
    expect(source('components/layout/SideSplit.tsx')).toContain('paddingTop: padTop');
    expect(source('components/layout/SideSplit.tsx')).toContain('pageRhythm.workbenchPadT');
    expect(source('components/ui/tabs.tsx')).not.toContain('mt-4 focus:outline-none');
    expect(source('pages/settings/index.tsx')).toContain('pageRhythm.chrome');
  });

  it('keeps page titles on the same type, height, and inset when switching pages', () => {
    const title = source('components/layout/PageHeader.tsx');
    const topBar = source('components/layout/TopBar.tsx');
    expect(title).toContain('pageRhythm.pageTitle');
    expect(title).toContain('pageRhythm.pageTitleBlock');
    expect(title).toContain('pageRhythm.pageTitleMeta');
    expect(title).toContain("shrink-0");
    expect(title).not.toContain("description || '\\u00a0'");
    expect(title).toContain('useRegisterPageChrome');
    expect(title).toContain('return null');
    expect(title).not.toContain('actions?:');
    expect(source('pages/connections/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/plugins/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/mcp/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/bridges/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/projects/index.tsx')).toContain('pageRhythm.chromeActions');
    expect(topBar).toContain('PageTitleBlock');
    expect(topBar).toContain('FeedbackButton');
    expect(topBar).toContain('NotificationBell');
    expect(topBar).toContain('pageRhythm.topChrome');
    expect(topBar).toContain('pageRhythm.workbenchX');
    expect(source('components/layout/Sidebar.tsx')).toContain('pageRhythm.topChrome');
    expect(source('pages/skills/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/projects/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/settings/index.tsx')).toContain('pageRhythm.workbenchHeader');
    expect(source('pages/settings/index.tsx')).toContain('toolbar={settingsTabList}');
    expect(source('pages/backups/BackupsPanel.tsx')).not.toContain('flushTop');
    expect(source('pages/backups/BackupsPanel.tsx')).toContain('pageRhythm.chromeActions');
    expect(source('pages/connections/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/bridges/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/plugins/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/agents/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('App.tsx')).toContain('!isChat && <TopBar');
    expect(source('App.tsx')).toContain('PageChromeProvider');
    expect(source('pages/settings/index.tsx')).not.toMatch(/readingColumn\}>\s*<PageHeader/);
    expect(source('pages/chat/ChatSessionHeader.tsx')).toContain('pageRhythm.chatChromeX');
    expect(source('pages/chat/ChatSessionHeader.tsx')).not.toContain('pageRhythm.workbenchHeader');
    expect(source('App.tsx')).toContain("pathname === '/connections'");
    expect(source('App.tsx')).toContain('isRoutesAreaPath');
    expect(source('App.tsx')).toContain('isRoutesArea');
    expect(source('App.tsx')).not.toContain("pathname === '/routes'");
    expect(source('App.tsx')).toContain("pathname === '/agents'");
    expect(source('App.tsx')).toContain("pathname === '/plugins'");
    expect(source('App.tsx')).toContain("pathname === '/settings'");
    expect(source('pages/routes/RoutesNav.tsx')).toContain('data-routes-nav');
    expect(source('pages/routes/RoutesLayout.tsx')).toContain('enterRoutesArea');
  });
});
