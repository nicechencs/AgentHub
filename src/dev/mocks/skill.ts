import type { SkillPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import { unsupportedError } from '@/lib/backend/contracts/errors';
import { isMappedState, mapMapStatus } from '@/lib/backend/contracts/skill-map';
import type {
  InstalledSkillDto,
  SkillListingDto,
} from '@/lib/backend/contracts/skill-types';
import {
  KNOWN_AGENT_IDS,
  type AgentId,
  type Skill,
  type SkillLinkKind,
  type SkillProjection,
  type SkillSyncState,
} from '@/lib/types';
import { loadJson } from '@/lib/ui-preferences';

/** Browser mock skills always have a shared source directory. */
type MockSkill = Omit<Skill, 'sourceDir'> & { sourceDir: string };

/** Last explicit projection mode per skill/agent; left-click reuses it (default copy). */
const lastProjectionMode = new Map<string, 'link' | 'copy'>();

function projectionModeKey(skillId: string, agentId: AgentId): string {
  return `${skillId}:${agentId}`;
}

function rememberProjectionMode(skillId: string, agentId: AgentId, mode: 'link' | 'copy') {
  lastProjectionMode.set(projectionModeKey(skillId, agentId), mode);
}

function lastStoredProjectionMode(skillId: string, agentId: AgentId): 'link' | 'copy' {
  return lastProjectionMode.get(projectionModeKey(skillId, agentId)) ?? 'copy';
}

/** Generic demo catalog — not a personal skill dump. */
const uniqueNames = [
  'notes', 'pdf', 'pdf-helper', 'git-commit', 'code-review', 'summarize', 'format-json',
  'search-docs', 'write-tests', 'changelog', 'open-url', 'extract-tables', 'draft-email',
  'find-skills', 'pptx', 'rename-files', 'lint-code', 'fix-typos', 'split-csv', 'merge-pdfs',
  'translate', 'outline', 'rewrite', 'cite-sources', 'compare-files', 'sort-list',
  'count-words', 'trim-whitespace', 'slugify', 'pretty-print', 'minify', 'convert-markdown',
  'generate-readme', 'scaffold', 'bump-version', 'tag-release', 'run-checks', 'parse-logs',
  'group-issues', 'prioritize', 'estimate', 'schedule', 'remind', 'bookmark', 'archive',
  'export-csv', 'import-csv', 'filter-rows', 'chart', 'diagram', 'screenshot',
  'resize-image', 'compress', 'hash-file', 'diff-text', 'wrap-text', 'clip-quote',
  'time-box', 'todo-list',
];

function seededRandom(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

const rand = seededRandom(42);
/** Fixed list — do not use runtime AGENT_IDS here: module init runs before catalog seed. */
const MOCK_AGENT_IDS: AgentId[] = [...KNOWN_AGENT_IDS];
const SKILL_CAPABLE: AgentId[] = MOCK_AGENT_IDS.filter((id) => id !== 'kimi');

function buildMockSkill(name: string): MockSkill {
  const sync = {} as Record<AgentId, SkillSyncState>;
  const conflicts: AgentId[] = [];
  const projections: SkillProjection[] = [];
  for (const agentId of MOCK_AGENT_IDS) {
    if (!SKILL_CAPABLE.includes(agentId)) {
      sync[agentId] = 'unsupported';
      projections.push({
        agent: agentId,
        state: 'unsupported',
        linkKind: 'none',
        targetDir: null,
        resolvedTarget: null,
        mapStatus: 'agent_unsupported',
      });
      continue;
    }
    const r = rand();
    let state: SkillSyncState;
    let linkKind: SkillLinkKind = 'none';
    if (r < 0.35) {
      state = 'linked';
      linkKind = 'junction';
    } else if (r < 0.62) {
      state = 'copied';
    } else if (r < 0.85) {
      state = 'absent';
    } else if (r < 0.95) {
      state = 'foreign';
      conflicts.push(agentId);
    } else {
      state = 'conflict';
      conflicts.push(agentId);
    }
    sync[agentId] = state;
    if (state === 'linked') rememberProjectionMode(name, agentId, 'link');
    else if (state === 'copied') rememberProjectionMode(name, agentId, 'copy');
    projections.push({
      agent: agentId,
      state,
      linkKind,
      targetDir: `C:\\mock\\.${agentId}\\skills\\${name}`,
      resolvedTarget: state === 'linked' ? `C:\\mock\\skills\\${name}` : null,
      mapStatus: mapMapStatus(undefined, state),
    });
  }
  return {
    id: name,
    name,
    description: `${name} demo skill`,
    sourceDir: `C:\\mock\\skills\\${name}`,
    projections,
    sync,
    conflicts,
  };
}

const INITIAL_MOCK_STATE = uniqueNames.map(buildMockSkill);
let mockState = structuredClone(INITIAL_MOCK_STATE);

function toSharedInstalledRow(s: MockSkill): InstalledSkillDto {
  return {
    id: s.id,
    name: s.name,
    description: s.description,
    sourceDir: s.sourceDir ?? `C:\\mock\\skills\\${s.id}`,
    rootLabel: '~/.agents/skills',
    rootDir: 'C:\\mock\\skills',
    origin: 'shared',
    projectable: true,
    mapStatus: 'available',
    source: null,
    projections: (s.projections ?? []) as InstalledSkillDto['projections'],
  };
}

const INITIAL_PRIVATE_SKILLS: InstalledSkillDto[] = [
  {
    id: 'sample-pet',
    name: 'sample-pet',
    description: 'Codex private skill',
    sourceDir: 'C:\\mock\\.codex\\skills\\sample-pet',
    rootLabel: '~/.codex/skills',
    rootDir: 'C:\\mock\\.codex\\skills',
    origin: 'codex',
    projectable: false,
    mapStatus: 'private_source',
    contentHash: 'sample-pet-identical',
    source: null,
    projections: [],
  },
  {
    id: 'sample-pet',
    name: 'sample-pet',
    description: 'Cursor private skill',
    sourceDir: 'C:\\mock\\.cursor\\skills-cursor\\sample-pet',
    rootLabel: '~/.cursor/skills-cursor',
    rootDir: 'C:\\mock\\.cursor\\skills-cursor',
    origin: 'cursor',
    projectable: false,
    mapStatus: 'private_source',
    contentHash: 'sample-pet-identical',
    source: null,
    projections: [],
  },
  {
    id: 'sample-changelog',
    name: 'sample-changelog',
    description: 'Claude private skill',
    sourceDir: 'C:\\mock\\.claude\\skills\\sample-changelog',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  {
    id: 'sample-review',
    name: 'sample-review',
    description: 'Claude private review skill',
    sourceDir: 'C:\\mock\\.claude\\skills\\sample-review',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  {
    id: 'sample-notes',
    name: 'sample-notes',
    description: 'Grok private notes skill',
    sourceDir: 'C:\\mock\\.grok\\skills\\sample-notes',
    rootLabel: '~/.grok/skills',
    rootDir: 'C:\\mock\\.grok\\skills',
    origin: 'grok',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  // Same id as a shared row, matching content → already in the library
  {
    id: 'notes',
    name: 'notes',
    description: 'notes demo skill',
    sourceDir: 'C:\\mock\\.claude\\skills\\notes',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'available',
    source: null,
    projections: [],
  },
  // Same id as a shared row, different content → conflict (can overwrite)
  {
    id: 'pdf',
    name: 'pdf',
    description: 'pdf demo skill',
    sourceDir: 'C:\\mock\\.claude\\skills\\pdf',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'conflict',
    source: null,
    projections: [],
  },
];

let mockPrivateSkills = structuredClone(INITIAL_PRIVATE_SKILLS);

function workspaceKey(path: string): string {
  return path.trim().replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

function mockProjectRow(
  id: string,
  name: string,
  description: string,
  workspace: string,
  origin = '.agents/skills',
): InstalledSkillDto {
  const rootDir = `${workspace.replace(/\\/g, '/')}/${origin}`;
  return {
    id,
    name,
    description,
    sourceDir: `${rootDir}/${id}`,
    rootLabel: origin,
    rootDir,
    origin,
    projectable: false,
    mapStatus: 'available',
    source: null,
    projections: [],
  };
}

const INITIAL_PROJECT_SKILLS: Record<string, InstalledSkillDto[]> = {
  [workspaceKey('C:\\Users\\demo\\app')]: [
    mockProjectRow('demo-notes', 'demo-notes', 'Project notes skill', 'C:\\Users\\demo\\app'),
  ],
  [workspaceKey('C:\\Users\\demo\\codex-work')]: [
    mockProjectRow('review', 'review', 'Codex work review skill', 'C:\\Users\\demo\\codex-work'),
  ],
};

let mockProjectSkills = new Map(
  Object.entries(structuredClone(INITIAL_PROJECT_SKILLS)),
);

/** Restores seeded skill catalog so each backend factory starts clean. */
export function resetMockSkills(): void {
  lastProjectionMode.clear();
  mockState = structuredClone(INITIAL_MOCK_STATE);
  mockPrivateSkills = structuredClone(INITIAL_PRIVATE_SKILLS);
  mockProjectSkills = new Map(Object.entries(structuredClone(INITIAL_PROJECT_SKILLS)));
}

export function createMockSkillPort(): SkillPort {
  return {
    async listSkills() {
      await delay(randomLatency());
      return mockState.map((s) => ({
        ...s,
        sync: { ...s.sync },
        conflicts: [...s.conflicts],
        projections: s.projections.map((p) => ({ ...p })),
      }));
    },

    async toggleSkillSync(skillId, agentId, opts = {}) {
      await delay(150);
      const skill = mockState.find((s) => s.id === skillId);
      if (!skill || skill.sync[agentId] === 'unsupported') {
        return { state: 'unsupported' as const, conflict: false };
      }
      const current = skill.sync[agentId];
      const next: SkillSyncState = opts.mode
        ? opts.mode === 'link'
          ? 'linked'
          : 'copied'
        : isMappedState(current)
          ? 'absent'
          : lastStoredProjectionMode(skillId, agentId) === 'link'
            ? 'linked'
            : 'copied';
      if (opts.mode) rememberProjectionMode(skillId, agentId, opts.mode);
      skill.sync[agentId] = next;
      const proj = skill.projections.find((p) => p.agent === agentId);
      if (proj) {
        proj.state = next;
        proj.linkKind = next === 'linked' ? 'junction' : 'none';
        if (isMappedState(next) && !proj.targetDir) {
          proj.targetDir = `~/.mock/${agentId}/skills/${skillId}`;
        }
      }
      skill.conflicts = skill.conflicts.filter((a) => a !== agentId);
      return { state: next, conflict: false };
    },

    async checkConflict(skillId, agentId) {
      await delay(100);
      const skill = mockState.find((s) => s.id === skillId);
      return !!skill && skill.conflicts.includes(agentId);
    },

    async syncAll() {
      await delay(1200);
      let synced = 0;
      for (const skill of mockState) {
        for (const agentId of SKILL_CAPABLE) {
          if (!isMappedState(skill.sync[agentId])) {
            skill.sync[agentId] = 'copied';
            const proj = skill.projections.find((p) => p.agent === agentId);
            if (proj) {
              proj.state = 'copied';
              proj.linkKind = 'none';
            }
            skill.conflicts = skill.conflicts.filter((a) => a !== agentId);
            synced++;
          }
        }
      }
      return { synced, skipped: Math.floor(rand() * 3), failed: 0 };
    },

    async listInstalledSkills() {
      await delay(randomLatency());
      const shared = mockState.map(toSharedInstalledRow);
      // 私有行与真源可同 id 并存（磁盘真相）；收编成功后再从 mockPrivateSkills 移除
      return [...shared, ...mockPrivateSkills.map((s) => ({ ...s }))];
    },

    async listSkillCatalog() {
      await delay(randomLatency());
      const shared = mockState.map(toSharedInstalledRow);
      const sharedIds = new Set(shared.map((s) => s.id));
      // 仅 private_source：已在共享库的 agent 副本不进 catalog（与 core list_catalog 一致）
      const privateOnly = mockPrivateSkills
        .filter((s) => s.mapStatus === 'private_source' && !sharedIds.has(s.id))
        .map((s) => ({ ...s, projections: [] as InstalledSkillDto['projections'] }));
      return [...shared, ...privateOnly];
    },

    async installSkillFromSource() {
      throw unsupportedError('技能安装（浏览器 mock）');
    },

    async importPrivateSkillToShared(skillId, agentId, overwrite = false) {
      await delay(200);
      const privateIdx = mockPrivateSkills.findIndex(
        (s) => s.id === skillId && s.origin === agentId,
      );
      if (privateIdx < 0) {
        throw new Error(`私有技能不存在: ${skillId} (${agentId})`);
      }
      const privateSkill = mockPrivateSkills[privateIdx];
      const exists = mockState.some((s) => s.id === skillId);
      if (exists && !overwrite) {
        throw new Error(
          `skill '${skillId}' already exists in shared source (pass overwrite to replace) [skill.conflict]`,
        );
      }
      const imported = buildMockSkill(skillId);
      imported.name = privateSkill.name;
      imported.description = privateSkill.description;
      if (exists) {
        const i = mockState.findIndex((s) => s.id === skillId);
        mockState[i] = imported;
      } else {
        mockState.push(imported);
      }
      // 收编后从「仅本地」列表移除（原目录在真实系统仍可保留；mock 用列表表示工作区）
      mockPrivateSkills.splice(privateIdx, 1);
      return imported;
    },

    async installSkill(source) {
      if (!source) throw unsupportedError('技能安装');
      await this.installSkillFromSource(source, false);
    },

    async uninstallSkill(skillId, privateAgent) {
      await delay(120);
      if (!privateAgent) {
        const i = mockState.findIndex((s) => s.id === skillId);
        if (i < 0) {
          throw new Error(`技能不存在: ${skillId}`);
        }
        mockState.splice(i, 1);
        return;
      }
      const idx = mockPrivateSkills.findIndex(
        (s) => s.id === skillId && s.origin === privateAgent,
      );
      if (idx < 0) {
        throw new Error(`私有技能不存在: ${skillId} (${privateAgent})`);
      }
      mockPrivateSkills.splice(idx, 1);
    },

    async updateSkill() {
      throw unsupportedError('技能更新（浏览器 mock）');
    },

    async applySkillProjection() {
      throw unsupportedError('技能映射（浏览器 mock）');
    },

    async searchSkillMarket(query = ''): Promise<SkillListingDto[]> {
      await delay(200);
      // Mock 模拟市场榜单（桌面端按设置走 skills.sh / skillhub.cn）
      const stored = loadJson<{ skillMarketSource?: string }>('agenthub:settings', {});
      const source = stored.skillMarketSource ?? 'auto';
      const useSkillhub = source === 'skillhub.cn';
      const skillsSh = [
        {
          id: 'vercel-labs/agent-skills/vercel-react-best-practices',
          name: 'vercel-react-best-practices',
          description: '来自 vercel-labs/agent-skills · 603.3K 次安装',
          version: null as string | null,
          providerId: 'skills.sh',
          installed: false,
          detailUrl:
            'https://skills.sh/vercel-labs/agent-skills/vercel-react-best-practices',
        },
        {
          id: 'anthropics/skills/frontend-design',
          name: 'frontend-design',
          description: '来自 anthropics/skills · 737.0K 次安装',
          version: null as string | null,
          providerId: 'skills.sh',
          installed: false,
          detailUrl: 'https://skills.sh/anthropics/skills/frontend-design',
        },
        {
          id: 'vercel-labs/skills/find-skills',
          name: 'find-skills',
          description: '来自 vercel-labs/skills · 2.8M 次安装',
          version: null as string | null,
          providerId: 'skills.sh',
          installed: true,
          detailUrl: 'https://skills.sh/vercel-labs/skills/find-skills',
        },
      ];
      const skillhub = [
        {
          id: 'skillhub:find-skills@1.0.0',
          name: 'Find Skills',
          description: '发现与安装 Agent Skills · 663.3K 次下载',
          version: '1.0.0',
          providerId: 'skillhub.cn',
          installed: true,
          detailUrl: 'https://skillhub.cn/skills/find-skills',
        },
        {
          id: 'skillhub:self-improving-agent@3.0.24',
          name: 'self-improving agent',
          description: '记录自身发现以实现自我改进 · 1.1M 次下载',
          version: '3.0.24',
          providerId: 'skillhub.cn',
          installed: false,
          // Official SPA route: /skills/{handle}/{slug}
          detailUrl: 'https://skillhub.cn/skills/pskoett/self-improving-agent',
        },
        {
          id: 'skillhub:react-best-practices@1.0.0',
          name: 'react-best-practices',
          description: 'React 实践 · skillhub.cn',
          version: '1.0.0',
          providerId: 'skillhub.cn',
          installed: false,
          detailUrl: 'https://skillhub.cn/skills/react-best-practices',
        },
      ];
      const list = useSkillhub ? skillhub : skillsSh;
      return list.filter(
        (x) =>
          !query ||
          x.name.toLowerCase().includes(query.toLowerCase()) ||
          x.id.toLowerCase().includes(query.toLowerCase()) ||
          x.description.toLowerCase().includes(query.toLowerCase()),
      );
    },

    async installMarketSkill(skillId, _overwrite = false) {
      await delay(400);
      const name = skillId.split('/').pop() ?? skillId;
      if (mockState.some((s) => s.id === name)) {
        return buildMockSkill(name);
      }
      const skill = buildMockSkill(name);
      mockState.push(skill);
      return skill;
    },

    async openPathInFileManager() {
      throw unsupportedError('打开目录');
    },

    onFsChanged() {
      return () => {};
    },

    async listProjectSkills(workspacePath) {
      await delay(randomLatency());
      const rows = mockProjectSkills.get(workspaceKey(workspacePath)) ?? [];
      return rows.map((row) => ({ ...row, projections: [] }));
    },

    async installProjectSkill(workspacePath, source, overwrite = false) {
      await delay(200);
      const key = workspaceKey(workspacePath);
      const trimmed = source.trim().replace(/[\\/]+$/, '');
      const id = trimmed.split(/[\\/]/).pop()?.replace(/\.git$/i, '') || 'skill';
      const rows = mockProjectSkills.get(key) ?? [];
      const exists = rows.some((row) => row.id === id);
      if (exists && !overwrite) {
        throw new Error(`skill '${id}' already exists (pass overwrite to replace)`);
      }
      const next = mockProjectRow(id, id, `${id} project skill`, workspacePath);
      const filtered = rows.filter((row) => row.id !== id);
      filtered.push(next);
      mockProjectSkills.set(key, filtered);
      return {
        id: next.id,
        name: next.name,
        description: next.description,
        sourceDir: next.sourceDir,
        projections: [],
      };
    },

    async uninstallProjectSkill(workspacePath, skillId, origin) {
      await delay(120);
      const key = workspaceKey(workspacePath);
      const rows = mockProjectSkills.get(key) ?? [];
      const idx = rows.findIndex(
        (row) => row.id === skillId && (!origin || row.origin === origin),
      );
      if (idx < 0) {
        throw new Error(`技能不存在: ${skillId}`);
      }
      rows.splice(idx, 1);
      mockProjectSkills.set(key, rows);
    },

    async readProjectSkillMarkdown(workspacePath, skillId, origin = null) {
      await delay(120);
      const key = workspaceKey(workspacePath);
      const rows = mockProjectSkills.get(key) ?? [];
      const hit = rows.find(
        (row) => row.id === skillId && (!origin || row.origin === origin),
      );
      if (!hit) {
        throw new Error(`技能不存在: ${skillId}`);
      }
      const content = [
        '---',
        `name: ${hit.name}`,
        `description: ${hit.description}`,
        '---',
        '',
        `# ${hit.name}`,
        '',
        hit.description,
        '',
        'This is a mock project skill.',
      ].join('\n');
      return {
        skillId: hit.id,
        name: hit.name,
        path: `${hit.sourceDir}/SKILL.md`,
        content,
        truncated: false,
      };
    },

    async readSkillMarkdown(skillId, privateAgent = null) {
      await delay(120);
      const fromShared = mockState.find((s) => s.id === skillId);
      const fromPrivate = mockPrivateSkills.find(
        (s) => s.id === skillId && (!privateAgent || s.origin === privateAgent),
      );
      const hit = privateAgent ? fromPrivate ?? fromShared : fromShared ?? fromPrivate;
      if (!hit) {
        throw new Error(`技能不存在: ${skillId}`);
      }
      const name = hit.name;
      const description = hit.description || `${name} demo skill`;
      const content = [
        '---',
        `name: ${name}`,
        `description: ${description}`,
        '---',
        '',
        `# ${name}`,
        '',
        description,
        '',
        '## 何时使用',
        '',
        `- 需要调用 **${name}** 相关能力时`,
        '- 在对话中明确提到该技能的目标场景时',
        '',
        '## 步骤',
        '',
        '1. 确认输入与前置条件',
        '2. 按技能约定执行主流程',
        '3. 输出结果并校验',
        '',
        '## 示例',
        '',
        '```bash',
        `# mock preview for ${skillId}`,
        'echo "skill ready"',
        '```',
        '',
        '| 字段 | 说明 |',
        '| --- | --- |',
        `| id | \`${skillId}\` |`,
        `| origin | ${privateAgent ?? 'shared'} |`,
        '',
        '> 这是浏览器 mock 预览内容，桌面端会读取真实 `SKILL.md`。',
        '',
      ].join('\n');
      return {
        skillId,
        name,
        path: `${hit.sourceDir}\\SKILL.md`,
        content,
        truncated: false,
      };
    },
  };
}
