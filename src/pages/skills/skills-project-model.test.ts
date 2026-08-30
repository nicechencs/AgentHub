import { describe, expect, it } from 'vitest';
import type { AgentProject } from '@/lib/types';
import type { InstalledSkillDto } from '@/lib/api/skill';
import { parseSkillTab } from './skills-preview-model';
import {
  filterProjectSkillRows,
  matchProjectSkillOption,
  projectSkillOptions,
  projectSkillRowKey,
} from './skills-project-model';

function project(partial: Partial<AgentProject> & Pick<AgentProject, 'id' | 'title'>): AgentProject {
  return {
    agentId: 'claude',
    storagePath: 'C:\\Users\\demo\\.claude\\projects\\x',
    actualPath: 'C:\\Users\\demo\\app',
    relativePath: 'projects/x',
    sessionCount: 1,
    sizeBytes: 1,
    updatedAt: '2026-08-01T00:00:00.000Z',
    ...partial,
  };
}

describe('projectSkillOptions', () => {
  it('keeps unique workspaces and skips rows without an openable path', () => {
    const rows: AgentProject[] = [
      project({ id: 'claude:proj:app', title: 'app', actualPath: 'C:\\Users\\demo\\app' }),
      project({
        id: 'codex:proj:app',
        agentId: 'codex',
        title: 'app-codex',
        actualPath: 'C:/Users/demo/app',
      }),
      project({ id: 'kimi:proj:none', title: 'ungrouped', actualPath: null }),
      project({
        id: 'grok:proj:perf',
        agentId: 'grok',
        title: 'perf',
        alias: 'Performance',
        actualPath: 'C:\\Users\\demo\\perf',
      }),
    ];
    const options = projectSkillOptions(rows);
    expect(options.map((o) => o.label)).toEqual(['app', 'Performance']);
    expect(options[0]?.workspacePath.replace(/\//g, '\\')).toBe('C:\\Users\\demo\\app');
  });

  it('prefers alias over title', () => {
    const options = projectSkillOptions([
      project({ id: 'x', title: 'raw', alias: 'Nice name', actualPath: 'D:\\work\\repo' }),
    ]);
    expect(options).toHaveLength(1);
    expect(options[0]?.label).toBe('Nice name');
  });
});

describe('matchProjectSkillOption', () => {
  it('matches slash and backslash variants', () => {
    const options = projectSkillOptions([
      project({ id: 'x', title: 'app', actualPath: 'C:\\Users\\demo\\app' }),
    ]);
    expect(matchProjectSkillOption(options, 'C:/Users/demo/app')?.label).toBe('app');
    expect(matchProjectSkillOption(options, null)).toBeNull();
  });
});

describe('filterProjectSkillRows', () => {
  const rows: InstalledSkillDto[] = [
    {
      id: 'notes',
      name: 'Notes',
      description: 'Take notes',
      sourceDir: 'C:/ws/.agents/skills/notes',
      rootLabel: '.agents/skills',
      rootDir: 'C:/ws/.agents/skills',
      origin: '.agents/skills',
      projectable: false,
      mapStatus: 'available',
      projections: [],
    },
    {
      id: 'review',
      name: 'Review',
      description: 'Code review',
      sourceDir: 'C:/ws/.claude/skills/review',
      rootLabel: '.claude/skills',
      rootDir: 'C:/ws/.claude/skills',
      origin: '.claude/skills',
      projectable: false,
      mapStatus: 'available',
      projections: [],
    },
  ];

  it('filters by name, id, description, or folder label', () => {
    expect(filterProjectSkillRows(rows, 'note').map((r) => r.id)).toEqual(['notes']);
    expect(filterProjectSkillRows(rows, '.claude').map((r) => r.id)).toEqual(['review']);
    expect(filterProjectSkillRows(rows, '  ').map((r) => r.id)).toEqual(['notes', 'review']);
  });
});

describe('parseSkillTab', () => {
  it('accepts project and keeps library as default', () => {
    expect(parseSkillTab('project')).toBe('project');
    expect(parseSkillTab('library')).toBe('library');
    expect(parseSkillTab('workspace')).toBe('library');
    expect(parseSkillTab(null)).toBe('library');
  });
});

describe('projectSkillRowKey', () => {
  it('includes origin so same id in two folders stay distinct', () => {
    expect(projectSkillRowKey({ origin: '.agents/skills', id: 'notes' })).toBe(
      'project:.agents/skills:notes',
    );
    expect(projectSkillRowKey({ origin: '.claude/skills', id: 'notes' })).toBe(
      'project:.claude/skills:notes',
    );
  });
});
