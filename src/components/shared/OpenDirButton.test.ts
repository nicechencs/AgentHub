import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('OpenDirButton', () => {
  it('only has icon-only and labeled directory modes', () => {
    const src = source('components/shared/OpenDirButton.tsx');
    expect(src).toContain("t('common.directory')");
    expect(src).toContain('size="icon"');
    expect(src).toContain('size="sm"');
    expect(src).toContain('labeled');
    expect(src).not.toContain('打开配置目录');
    expect(src).not.toContain('打开安装目录');
    expect(src).not.toContain('打开目录');
  });

  it('is used for path-row and toolbar open-folder actions', () => {
    expect(source('pages/agents/AgentDetailPanel.tsx')).toContain('<OpenDirButton');
    expect(source('pages/agents/AgentDetailPanel.tsx')).toContain('labeled');
    expect(source('pages/plugins/PluginDetailPanel.tsx')).toContain('<OpenDirButton');
    expect(source('pages/plugins/PluginDetailPanel.tsx')).toContain('labeled');
    expect(source('pages/mcp/McpServerTable.tsx')).toContain('<OpenDirButton');
    expect(source('pages/mcp/McpServerTable.tsx')).toContain('labeled');
    expect(source('components/shared/ConfigFileCard.tsx')).toContain('<OpenDirButton');
    expect(source('components/shared/ConfigFileCard.tsx')).toContain('labeled');
    expect(source('components/connections/ProviderEditDialog.tsx')).toContain('<OpenDirButton');
    expect(source('components/connections/ProviderEditDialog.tsx')).toContain('labeled');
    expect(source('pages/settings/LocalPanel.tsx')).toContain('<OpenDirButton');
    expect(source('pages/settings/LocalPanel.tsx')).toContain('labeled');
    expect(source('pages/skills/SkillMarkdownPreviewPanel.tsx')).toContain('<OpenDirButton');
    expect(source('pages/skills/SkillMarkdownPreviewPanel.tsx')).not.toContain('labeled');
    expect(source('pages/projects/ProjectConversationPreviewPanel.tsx')).toContain('<OpenDirButton');
    expect(source('pages/projects/ProjectConversationPreviewPanel.tsx')).not.toContain('labeled');
  });

  it('keeps FolderOpen off ad-hoc open-directory buttons', () => {
    const buttonFiles = [
      'pages/agents/AgentDetailPanel.tsx',
      'pages/plugins/PluginDetailPanel.tsx',
      'pages/mcp/McpServerTable.tsx',
      'components/shared/ConfigFileCard.tsx',
      'components/connections/ProviderEditDialog.tsx',
      'pages/settings/LocalPanel.tsx',
      'pages/skills/SkillMarkdownPreviewPanel.tsx',
      'pages/projects/ProjectConversationPreviewPanel.tsx',
    ];
    for (const rel of buttonFiles) {
      expect(source(rel), rel).not.toContain('FolderOpen');
    }
    expect(source('pages/skills/SkillMatrix.tsx')).toContain('FolderOpen');
    expect(source('pages/chat/ChatSettingsDialog.tsx')).toContain('FolderOpen');
    expect(source('pages/chat/ChatSessionHeader.tsx')).toContain('FolderOpen');
  });
});
