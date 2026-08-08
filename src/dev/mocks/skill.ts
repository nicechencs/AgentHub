import { AGENT_IDS } from '@/config/agents';
import type { SkillPort } from '@/lib/backend/contracts';
import { delay, randomLatency } from '@/dev/mocks/delay';
import { unsupportedError } from '@/lib/backend/contracts/errors';
import { isMappedState, mapMapStatus } from '@/lib/backend/contracts/skill-map';
import type {
  InstalledSkillDto,
  SkillListingDto,
} from '@/lib/backend/contracts/skill-types';
import type {
  AgentId,
  Skill,
  SkillLinkKind,
  SkillProjection,
  SkillSyncState,
} from '@/lib/types';
import { loadJson } from '@/lib/ui-preferences';

/** Browser mock skills always have a shared source directory. */
type MockSkill = Omit<Skill, 'sourceDir'> & { sourceDir: string };

const dbsNames = [
  'action', 'agent-migration', 'ai-check', 'benchmark', 'chatroom', 'content', 'deconstruct',
  'diagnosis', 'goal', 'good-question', 'hook', 'learning', 'report', 'restore', 'save',
  'slowisfast', 'xhs-title',
];
const hfNames = ['core', 'cli', 'animation', 'keyframes', 'creative', 'registry'];
const larkNames = [
  'approval', 'apps', 'attendance', 'base', 'calendar', 'contact', 'doc', 'drive', 'event',
  'im', 'mail', 'markdown', 'minutes', 'note', 'okr', 'openapi-explorer', 'shared', 'sheets',
  'skill-maker', 'slides', 'task', 'vc', 'vc-agent', 'whiteboard', 'wiki',
  'workflow-meeting-summary', 'workflow-standup-report',
];
const miscNames = [
  'agent-builder', 'find-skills', 'pdf', 'pptx', 'media-use', 'general-video',
  'write-goal', 'update-config', 'check-docs',
];

const skillNames: string[] = [
  ...dbsNames.map((n) => `dbs-${n}`),
  ...hfNames.map((n) => `hyperframes-${n}`),
  'hyperframes',
  ...larkNames.map((n) => `lark-${n}`),
  ...miscNames,
];
const uniqueNames = [...new Set(skillNames)].slice(0, 59);

function seededRandom(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

const rand = seededRandom(42);
const SKILL_CAPABLE: AgentId[] = AGENT_IDS.filter((id) => id !== 'kimi');

function buildMockSkill(name: string): MockSkill {
  const sync = {} as Record<AgentId, SkillSyncState>;
  const conflicts: AgentId[] = [];
  const projections: SkillProjection[] = [];
  for (const agentId of AGENT_IDS) {
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
    description: `${name} 技能`,
    sourceDir: `C:\\mock\\skills\\${name}`,
    projections,
    sync,
    conflicts,
  };
}

const mockState: MockSkill[] = uniqueNames.map(buildMockSkill);

const mockPrivateSkills: InstalledSkillDto[] = [
  {
    id: 'hatch-pet',
    name: 'hatch-pet',
    description: 'Codex private skill',
    sourceDir: 'C:\\mock\\.codex\\skills\\hatch-pet',
    rootLabel: '~/.codex/skills',
    rootDir: 'C:\\mock\\.codex\\skills',
    origin: 'codex',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  {
    id: 'changelog-generator',
    name: 'changelog-generator',
    description: 'Claude private skill',
    sourceDir: 'C:\\mock\\.claude\\skills\\changelog-generator',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  {
    id: 'local-review',
    name: 'local-review',
    description: 'Claude 本地 code review 流程',
    sourceDir: 'C:\\mock\\.claude\\skills\\local-review',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  {
    id: 'grok-session-notes',
    name: 'grok-session-notes',
    description: 'Grok 会话纪要（仅本地）',
    sourceDir: 'C:\\mock\\.grok\\skills\\grok-session-notes',
    rootLabel: '~/.grok/skills',
    rootDir: 'C:\\mock\\.grok\\skills',
    origin: 'grok',
    projectable: false,
    mapStatus: 'private_source',
    source: null,
    projections: [],
  },
  // 与共享库同 id 且内容一致 → 已在共享库
  {
    id: 'dbs-action',
    name: 'dbs-action',
    description: '已同步到 Claude 的共享技能（内容一致）',
    sourceDir: 'C:\\mock\\.claude\\skills\\dbs-action',
    rootLabel: '~/.claude/skills',
    rootDir: 'C:\\mock\\.claude\\skills',
    origin: 'claude',
    projectable: false,
    mapStatus: 'available',
    source: null,
    projections: [],
  },
  // 与共享库同 id 但内容不同 → 内容不同（可覆盖加入）
  {
    id: 'pdf',
    name: 'pdf',
    description: 'Claude 本地改过，与共享库内容不同',
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

    async toggleSkillSync(skillId, agentId, _opts = {}) {
      await delay(150);
      const skill = mockState.find((s) => s.id === skillId);
      if (!skill || skill.sync[agentId] === 'unsupported') {
        return { state: 'unsupported' as const, conflict: false };
      }
      const next: SkillSyncState = isMappedState(skill.sync[agentId]) ? 'absent' : 'copied';
      skill.sync[agentId] = next;
      const proj = skill.projections.find((p) => p.agent === agentId);
      if (proj) {
        proj.state = next;
        proj.linkKind = 'none';
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
      const shared = mockState.map((s) => ({
        id: s.id,
        name: s.name,
        description: s.description,
        sourceDir: s.sourceDir ?? `C:\\mock\\skills\\${s.id}`,
        rootLabel: '~/.agents/skills',
        rootDir: 'C:\\mock\\skills',
        origin: 'shared',
        projectable: true,
        mapStatus: 'available' as const,
        source: null,
        projections: (s.projections ?? []) as InstalledSkillDto['projections'],
      }));
      // 私有行与真源可同 id 并存（磁盘真相）；收编成功后再从 mockPrivateSkills 移除
      return [...shared, ...mockPrivateSkills.map((s) => ({ ...s }))];
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

    async uninstallSkill() {
      throw unsupportedError('技能卸载（浏览器 mock）');
    },

    async updateSkill() {
      throw unsupportedError('技能更新（浏览器 mock）');
    },

    async projectSkill() {
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
      const description = hit.description || `${name} 技能`;
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
