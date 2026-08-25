import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

function workbenchHeaderWrapper(pageSrc: string): string {
  const compactAt = pageSrc.indexOf('size="compact"');
  expect(compactAt).toBeGreaterThan(0);
  const wrapperAt = pageSrc.lastIndexOf('<div', compactAt);
  expect(wrapperAt).toBeGreaterThanOrEqual(0);
  return pageSrc.slice(wrapperAt, compactAt);
}

describe('PageHeader', () => {
  it('does not draw a rule under the title; Skills/Projects match other pages', () => {
    expect(source('components/layout/PageHeader.tsx')).not.toContain('border-b');
    expect(workbenchHeaderWrapper(source('pages/skills/index.tsx'))).not.toContain('border-b');
    expect(workbenchHeaderWrapper(source('pages/projects/index.tsx'))).not.toContain('border-b');
    expect(source('components/layout/SideSplit.tsx')).toContain('pageRhythm.workbenchHeader');
    expect(source('components/layout/SideSplit.tsx')).not.toContain('border-b');
    expect(source('pages/connections/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/bridges/index.tsx')).toContain('WorkbenchSplitPage');
  });

  it('starts non-Chat body flush under the title slot', () => {
    expect(source('components/layout/PageHeader.tsx')).toContain("compact ? 'mb-0' : 'mb-[18px]'");
    expect(source('components/layout/page-rhythm.ts')).toContain("workbenchY: 'pb-[18px]'");
    expect(source('pages/skills/index.tsx')).toContain('pageRhythm.chrome');
    expect(source('pages/skills/index.tsx')).not.toContain('className="mb-2"');
    expect(source('pages/skills/index.tsx')).toContain('paddingTop: 0');
    expect(source('pages/projects/index.tsx')).toContain('paddingTop: 0');
    expect(source('components/ui/tabs.tsx')).not.toContain('mt-4 focus:outline-none');
    expect(source('pages/settings/index.tsx')).toContain('pageRhythm.chrome');
  });

  it('keeps page titles on the same type, height, and inset when switching pages', () => {
    const header = source('components/layout/PageHeader.tsx');
    expect(header).toContain('pageRhythm.pageTitle');
    expect(header).toContain('pageRhythm.pageTitleBlock');
    expect(header).toContain("description || '\\u00a0'");
    expect(source('pages/skills/index.tsx')).toContain('pageRhythm.workbenchHeader');
    expect(source('pages/projects/index.tsx')).toContain('pageRhythm.workbenchHeader');
    expect(source('components/layout/SideSplit.tsx')).toContain('pageRhythm.workbenchHeader');
    expect(source('pages/connections/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('pages/bridges/index.tsx')).toContain('WorkbenchSplitPage');
    expect(source('App.tsx')).toContain('!isChat && <TopBar');
    expect(source('pages/settings/index.tsx')).not.toMatch(/readingColumn\}>\s*<PageHeader/);
    expect(source('pages/chat/ChatSessionHeader.tsx')).toContain('pageRhythm.chatChromeX');
    expect(source('pages/chat/ChatSessionHeader.tsx')).not.toContain('pageRhythm.workbenchHeader');
    expect(source('App.tsx')).toContain("pathname === '/connections'");
    expect(source('App.tsx')).toContain("pathname === '/routes'");
  });
});
