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

  it('uses the split-list inset when a preview pane is mounted', () => {
    expect(source('pages/projects/index.tsx')).toContain('workbenchXSplit');
    expect(source('pages/skills/index.tsx')).toContain('workbenchXSplit');
    expect(source('components/layout/SideSplit.tsx')).toContain('workbenchXSplit');
  });

  it('keeps workbench header actions in the list column, left of the separator', () => {
    const split = source('components/layout/SideSplit.tsx');
    const pageFn = split.slice(split.indexOf('export function WorkbenchSplitPage'));
    const splitRefAt = pageFn.indexOf('split.splitRef');
    const listColAt = pageFn.indexOf('flex min-h-0 min-w-0 flex-1 flex-col');
    const headerAt = pageFn.indexOf('pageRhythm.workbenchHeader');
    const inspectAt = pageFn.indexOf('split.mounted && panel');
    expect(splitRefAt).toBeGreaterThan(0);
    expect(listColAt).toBeGreaterThan(splitRefAt);
    expect(headerAt).toBeGreaterThan(listColAt);
    expect(inspectAt).toBeGreaterThan(headerAt);

    const projects = source('pages/projects/index.tsx');
    const projectsSplit = projects.indexOf('preview.splitRef');
    const projectsListCol = projects.indexOf('flex min-h-0 min-w-0 flex-1 flex-col');
    const projectsHeader = projects.indexOf('pageRhythm.workbenchHeader');
    const projectsSep = projects.indexOf('role="separator"');
    expect(projectsSplit).toBeGreaterThan(0);
    expect(projectsListCol).toBeGreaterThan(projectsSplit);
    expect(projectsHeader).toBeGreaterThan(projectsListCol);
    expect(projectsSep).toBeGreaterThan(projectsHeader);
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
