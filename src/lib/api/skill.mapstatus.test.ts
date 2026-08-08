import { describe, expect, it } from 'vitest';
import {
  isActionableMapStatus,
  isPrivateInstalledOrigin,
  mapCoreSkill,
  mapStatusLabel,
  resolveWorkspacePresence,
  workspacePresenceLabel,
  type CoreSkill,
} from './skill';

describe('skill mapStatus helpers', () => {
  it('labels blocked and conflict reasons clearly', () => {
    expect(mapStatusLabel('private_source')).toBe('只在本工具里，需先加入共享库');
    expect(mapStatusLabel('agent_unsupported')).toBe('该工具不支持技能');
    expect(mapStatusLabel('agent_not_installed')).toBe('该工具尚未安装');
    expect(mapStatusLabel('target_unavailable')).toBe('技能目录不可用');
    expect(mapStatusLabel('conflict')).toBe('可启用，但目标已有不同内容');
  });

  it('workspace badges use short labels and presence classes', () => {
    expect(workspacePresenceLabel('claude', 'private_source')).toBe('只在本工具');
    expect(workspacePresenceLabel('codex', 'conflict')).toBe('内容不同');
    expect(workspacePresenceLabel('claude', 'available')).toBe('已在共享库');
    expect(workspacePresenceLabel('shared', 'available')).toBe('共享库');
    expect(isPrivateInstalledOrigin('claude')).toBe(true);
    expect(isPrivateInstalledOrigin('shared')).toBe(false);
    expect(resolveWorkspacePresence('claude', 'private_source')).toBe('private_only');
    expect(resolveWorkspacePresence('claude', 'available')).toBe('in_library');
    expect(resolveWorkspacePresence('claude', 'conflict')).toBe('conflict');
    expect(resolveWorkspacePresence('shared', 'available')).toBe('shared');
  });

  it('treats conflict as actionable, private/unsupported as blocked', () => {
    expect(isActionableMapStatus('available')).toBe(true);
    expect(isActionableMapStatus('conflict')).toBe(true);
    expect(isActionableMapStatus('private_source')).toBe(false);
    expect(isActionableMapStatus('agent_unsupported')).toBe(false);
    expect(isActionableMapStatus('target_unavailable')).toBe(false);
  });

  it('maps core projections including mapStatus for kimi and conflict', () => {
    const core: CoreSkill = {
      id: 'demo',
      name: 'Demo',
      description: 'd',
      sourceDir: 'C:\\skills\\demo',
      projections: [
        {
          agent: 'claude',
          state: 'absent',
          linkKind: 'none',
          targetDir: 'C:\\claude\\skills\\demo',
          mapStatus: 'available',
        },
        {
          agent: 'kimi',
          state: 'unsupported',
          linkKind: 'none',
          targetDir: null,
          mapStatus: 'agent_unsupported',
        },
        {
          agent: 'grok',
          state: 'foreign',
          linkKind: 'none',
          targetDir: 'C:\\grok\\skills\\demo',
          mapStatus: 'conflict',
        },
      ],
    };
    const ui = mapCoreSkill(core);
    expect(ui.sync.claude).toBe('absent');
    expect(ui.projections.find((p) => p.agent === 'claude')?.mapStatus).toBe('available');
    expect(ui.projections.find((p) => p.agent === 'kimi')?.mapStatus).toBe('agent_unsupported');
    expect(ui.projections.find((p) => p.agent === 'grok')?.mapStatus).toBe('conflict');
    expect(ui.conflicts).toContain('grok');
  });
});
