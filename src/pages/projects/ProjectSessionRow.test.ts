import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { projectGroupCardGrid } from './ProjectTree';
import { projectSessionRowGrid } from './ProjectSessionRow';

const dir = path.dirname(fileURLToPath(import.meta.url));
const src = readFileSync(path.join(dir, 'ProjectSessionRow.tsx'), 'utf8');
const tree = readFileSync(path.join(dir, 'ProjectTree.tsx'), 'utf8');

describe('ProjectSessionRow', () => {
  it('opens preview from the title and reveals the record from the file-name field', () => {
    expect(src).toContain('sessionFileName');
    expect(src).toContain('onPreviewSession(session)');
    expect(src).toContain('followPreview');
    expect(src).toContain('shouldOpenTableRowFromClick');
    expect(src).toContain('onOpenSessionRecord(session, e)');
    expect(src).not.toContain('ProjectPathLink');
    expect(src).not.toContain('FolderOpen');
  });

  it('shows a disabled delete control when deleteHint is set', () => {
    expect(src).toContain('deleteHint');
    expect(src).toContain('showDeleteAction');
    expect(src).toContain('disabled={busy || Boolean(deleteHint)}');
    expect(src).toContain('if (deleteHint) return;');
  });

  it('keeps the action cluster on an auto track so icons cannot overlap the file name', () => {
    for (const grid of [
      projectSessionRowGrid(true),
      projectSessionRowGrid(false),
      projectSessionRowGrid(true, true),
      projectSessionRowGrid(false, true),
    ]) {
      expect(grid).toContain('minmax(0,1fr)_auto');
      expect(grid).toContain('overflow-hidden');
    }
    expect(projectSessionRowGrid(true, true)).toContain('1.25rem_1.5rem_minmax(0,22rem)');
    expect(src).toContain('showAgent');
    expect(src).toContain('AgentLogo');
    expect(src).toContain('size="sm"');
    expect(src).not.toContain('AgentDot');
  });

  it('aligns fields on a borderless grid and keeps continue as an icon', () => {
    expect(src).toContain('projectSessionRowGrid');
    expect(src).toContain('grid-cols-');
    expect(src).not.toContain('divide-y');
    expect(src).not.toContain('border-t');
    expect(src).toContain('projects.tree.openRecordFolder');
    expect(src).toContain('projects.tree.copySessionId');
    expect(src).toContain('MessageSquarePlus');
    expect(src).not.toMatch(/size="sm"[\s\S]{0,200}projects\.tree\.continue/);
  });

  it('keeps project cards and collapsible groups, without session row dividers', () => {
    expect(tree).toContain("from '@/components/ui/card'");
    expect(tree).toContain('aria-expanded');
    expect(tree).toContain('onToggleExpand');
    expect(tree).toContain('bg-subtle/40');
    expect(tree).not.toContain('divide-y');
  });

  it('splits project card fields onto separate tracks instead of one packed meta string', () => {
    expect(projectGroupCardGrid()).toContain('grid-cols-subgrid');
    expect(tree).toContain('projectGroupListGrid');
    expect(tree).toContain('projectGroupListTemplate');
    expect(tree).toContain('useColumnWidths');
    expect(tree).toContain('ColumnResizeHandle');
    expect(tree).toContain('COLUMN_WIDTHS_STORAGE_KEY');
    expect(tree).toContain('grid-cols-subgrid');
    expect(tree).toContain('projectTitleHoverLabel');
    expect(tree).toContain("t('projects.tree.sessionCount'");
    expect(tree).toContain('fmtBytes(group.sizeBytes)');
    expect(tree).toContain('relativeTime(group.updatedAt, t)');
    expect(tree).not.toContain('sessionMeta');
    expect(tree).not.toContain('({p.title})');
    expect(tree).not.toContain('flex min-w-0 flex-1 items-center gap-2');
    expect(tree).not.toContain('min-w-0 flex-1');
    expect(tree).not.toContain('pageRhythm.stackDense');
  });
});

