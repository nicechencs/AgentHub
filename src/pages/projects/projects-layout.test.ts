import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('projects split layout', () => {
  it('opens session preview in the shared workbench inspect pane', () => {
    const page = source('index.tsx');
    expect(page).toContain('WorkbenchSplitPage');
    expect(page).toContain('useSideSplit');
    expect(page).toContain('PageHeader');
    expect(page).toContain('pageRhythm.chromeActions');
    expect(page).toContain("t('projects.preview.resizeAria')");
    expect(page).toContain('listOverflowX="hidden"');
    expect(page).toContain('<ProjectConversationPreviewPanel');
    expect(page).not.toContain('useProjectPreview');
    expect(page).not.toContain('previewShellMounted');
    expect(page).toContain('followPreview={preview.expanded}');
  });

  it('offers 全部, merges same-path projects, and sorts the list', () => {
    const page = source('index.tsx');
    expect(page).toContain('showAll');
    expect(page).toContain("t('kind.all')");
    expect(page).toContain('groupProjectsByPath');
    expect(page).toContain('sortProjectGroups');
    expect(page).toContain("t('projects.page.sortTime')");
    expect(page).toContain("t('projects.page.sortAgent')");
    expect(page).toContain("t('projects.page.sortName')");
    const tree = source('ProjectTree.tsx');
    expect(tree).toContain('showSessionAgent');
    expect(tree).toContain('group.agentIds');
    expect(tree).toContain('AgentLogo');
    expect(tree).not.toContain('AgentDot');
    expect(tree).toContain('sessionPageCount');
    expect(tree).toContain("t('projects.tree.pageNext'");
    expect(tree).toContain('justify-start gap-2 border-t');
    expect(tree).toContain('text-xs text-accent tabular-nums');
    expect(tree).toContain('className="text-accent hover:text-accent"');
    expect(tree).toContain('hidden={!open}');
    expect(tree).toContain('projectGroupListGrid');
    expect(tree).toContain('projectGroupCardGrid');
    expect(tree).toContain('projectGroupListTemplate');
    expect(tree).toContain('ColumnResizeHandle');
    expect(tree).toContain('grid-cols-subgrid');
    expect(tree).toContain("t('projects.tree.sessionCount'");
    expect(tree).not.toContain('sessionMeta');
  });

  it('renders excerpt turns as a user/assistant conversation', () => {
    const preview = source('ProjectConversationPreviewPanel.tsx');
    expect(preview).toContain("t('projects.preview.roleUser')");
    expect(preview).toContain("t('projects.preview.roleAssistant'");
    expect(preview).toContain('justify-end');
    expect(preview).toContain('justify-start');
    expect(preview).toContain('<AgentDot');
    expect(preview).toContain('rounded-composer bg-subtle');
    expect(preview).toContain('rounded-composer bg-hover/60');
    expect(preview).toContain("t('projects.preview.copyRecord')");
    expect(preview).toContain("t('projects.preview.copyTurn')");
    expect(preview).toContain("t('projects.preview.truncated')");
    expect(preview).toContain("t('projects.preview.convention')");
    expect(preview).toContain("t('projects.preview.approvals')");
    expect(preview).toContain("t('projects.preview.backToConversation')");
    expect(preview).toContain('CopyTextButton');
    expect(preview).toContain('reloadKey');
  });
});
