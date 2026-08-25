import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { projectSessionRowGrid } from './ProjectSessionRow';

const dir = path.dirname(fileURLToPath(import.meta.url));
const src = readFileSync(path.join(dir, 'ProjectSessionRow.tsx'), 'utf8');
const tree = readFileSync(path.join(dir, 'ProjectTree.tsx'), 'utf8');

describe('ProjectSessionRow', () => {
  it('opens preview from the title and reveals the record from the file-name field', () => {
    expect(src).toContain('sessionFileName');
    expect(src).toContain('onPreviewSession(session)');
    expect(src).toContain('onOpenSessionRecord(session, e)');
    expect(src).not.toContain('ProjectPathLink');
    expect(src).not.toContain('FolderOpen');
  });

  it('keeps the action cluster on an auto track so icons cannot overlap the file name', () => {
    for (const grid of [projectSessionRowGrid(true), projectSessionRowGrid(false)]) {
      expect(grid).toContain('minmax(0,1fr)_auto');
      expect(grid).toContain('overflow-hidden');
    }
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
});

