import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(dir, '../..');

function source(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

describe('CopyableFileName wiring', () => {
  it('is the path label on login files, backups, MCP, plugins, Agents, settings, and previews', () => {
    expect(source('components/shared/CopyableFileName.tsx')).toContain("t('common.copyFileName')");
    expect(source('components/shared/CopyableFileName.tsx')).toContain("t('common.copiedFileName'");
    expect(source('components/shared/CopyableFileName.tsx')).not.toContain('title={path');
    expect(source('components/shared/ConfigFileCard.tsx')).toContain('<CopyableFileName');
    expect(source('pages/mcp/McpServerTable.tsx')).toContain('<CopyableFileName');
    expect(source('pages/plugins/PluginDetailPanel.tsx')).toContain('<CopyableFileName');
    expect(source('pages/agents/AgentDetailPanel.tsx')).toContain('<CopyableFileName');
    expect(source('pages/skills/SkillMarkdownPreviewPanel.tsx')).toContain('<CopyableFileName');
    expect(source('pages/projects/ProjectConversationPreviewPanel.tsx')).toContain('<CopyableFileName');
    expect(source('pages/settings/LocalPanel.tsx')).toContain('<CopyableFileName');
    expect(source('components/connections/ProviderEditDialog.tsx')).toContain('<CopyableFileName');
    expect(source('pages/routes/shared/WriteClientConfigDialog.tsx')).toContain('<CopyableFileName');
  });
});
