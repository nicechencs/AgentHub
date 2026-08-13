import type { SkillPort } from '@/lib/backend/contracts';
import { mapCoreSkill, isMappedState } from '@/lib/backend/contracts/skill-map';
import type {
  CoreSkill,
  InstalledSkillDto,
  SkillListingDto,
  SkillMarkdownPreviewDto,
  SkillProjectResultDto,
  SkillSyncReport,
  SkillsFsChangedPayload,
} from '@/lib/backend/contracts/skill-types';
import { unsupportedError } from '@/lib/backend/contracts/errors';
import { onSkillsFsChanged } from './skill-events';
import { invoke } from './invoke';

export function createTauriSkillPort(): SkillPort {
  async function listSkillsMapped() {
    const rows = await invoke<CoreSkill[]>('list_skills');
    return rows.map(mapCoreSkill);
  }

  return {
    listSkills: listSkillsMapped,

    async toggleSkillSync(skillId, agentId, opts = {}) {
      const current = (await listSkillsMapped()).find((s) => s.id === skillId);
      if (!current || current.sync[agentId] === 'unsupported') {
        return { state: 'unsupported' as const, conflict: false };
      }
      const wasMapped = isMappedState(current.sync[agentId]);
      if (wasMapped) {
        await invoke('disable_skill', { skillId, agentId });
      } else {
        try {
          await invoke('sync_skill', {
            skillId,
            agentId,
            force: opts.force ?? false,
          });
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          if (msg.includes('skill.conflict') || msg.toLowerCase().includes('conflict')) {
            return { state: 'foreign' as const, conflict: true };
          }
          throw e instanceof Error ? e : new Error(msg);
        }
      }
      const after = (await listSkillsMapped()).find((s) => s.id === skillId);
      if (!after) throw new Error(`技能不存在: ${skillId}`);
      return {
        state: after.sync[agentId],
        conflict: after.conflicts.includes(agentId),
      };
    },

    async checkConflict(skillId, agentId) {
      const skills = await listSkillsMapped();
      const skill = skills.find((s) => s.id === skillId);
      return !!skill && skill.conflicts.includes(agentId);
    },

    async syncAll() {
      const report = await invoke<SkillSyncReport>('sync_all_skills', {
        agentId: null,
        force: false,
      });
      return {
        synced: report.synced.length,
        skipped: report.skipped.length,
        failed: report.failed.length,
      };
    },

    async listInstalledSkills() {
      return invoke<InstalledSkillDto[]>('list_installed_skills');
    },

    async installSkillFromSource(source, overwrite = false) {
      return invoke<CoreSkill>('install_skill', { source, overwrite });
    },

    async importPrivateSkillToShared(skillId, agentId, overwrite = false) {
      const row = await invoke<CoreSkill>('import_private_skill', {
        skillId,
        agentId,
        overwrite,
      });
      return mapCoreSkill(row);
    },

    async installSkill(source) {
      if (!source) throw unsupportedError('技能安装');
      await invoke<CoreSkill>('install_skill', { source, overwrite: false });
    },

    async uninstallSkill(skillId, privateAgent) {
      await invoke('uninstall_skill', {
        skillId,
        privateAgent: privateAgent ?? null,
      });
    },

    async updateSkill(skillId) {
      return invoke<CoreSkill>('update_skill', { skillId });
    },

    async projectSkill(skillId, agentId, mode: 'link' | 'copy' = 'link') {
      return invoke<SkillProjectResultDto>('project_skill', {
        skillId,
        agentId,
        mode,
      });
    },

    async searchSkillMarket(query = '') {
      return invoke<SkillListingDto[]>('search_skill_market', { query });
    },

    async installMarketSkill(skillId, overwrite = false) {
      return invoke<CoreSkill>('install_market_skill', { skillId, overwrite });
    },

    async openPathInFileManager(path) {
      return invoke<string>('open_path_in_file_manager', { path });
    },

    async readSkillMarkdown(skillId, privateAgent = null) {
      return invoke<SkillMarkdownPreviewDto>('read_skill_markdown', {
        skillId,
        privateAgent: privateAgent ?? null,
      });
    },

    onFsChanged(handler: (payload?: SkillsFsChangedPayload) => void) {
      return onSkillsFsChanged(handler);
    },
  };
}
