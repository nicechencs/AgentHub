import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { zh } from '@/lib/i18n/locales/zh';
import {
  agentTaskLogTitleKey,
  extraCopyKindLabel,
  extraCopyKindLabelKey,
  extraCopyUpdateHint,
  canInstallAlongsideSpecial,
  canUninstallProgramInApp,
  installLifecycle,
  installRetryButtonVariant,
  isInAppUpgradeChannel,
  listAgentInstalls,
  isNodeTooOldUpdateNote,
  isSpecialInstallChannel,
  specialChannelUpdateTargets,
  uniqueInstallVersions,
} from './agent-card-model';

const dir = path.dirname(fileURLToPath(import.meta.url));

describe('agent-card menu wiring', () => {
  it('keeps install/update/hide on the card and puts uninstall in the inspect pane', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    const detail = readFileSync(path.join(dir, 'AgentDetailPanel.tsx'), 'utf8');
    const dialogs = readFileSync(path.join(dir, 'AgentCardDialogs.tsx'), 'utf8');

    expect(card).toContain('canInstallAlongsideSpecial');
    expect(card).toContain('uniqueInstallVersions');
    expect(card).toContain("t('agents.card.seeDetails')");
    expect(card).not.toContain('openAgentCardUninstallConfirm');
    expect(card).not.toContain("t('agents.card.uninstallProgram')");
    expect(card).not.toContain("t('agents.card.uninstallConfig')");
    expect(card).not.toContain('openBinDir');
    expect(card).not.toContain('DropdownMenu');
    expect(detail).toContain('canUninstallProgramInApp');
    expect(detail).toContain("setConfirmDialog('program')");
    expect(detail).toContain("setConfirmDialog('config')");
    expect(detail).toContain('openPathInFileManager');
    expect(detail).toContain('installs.map');
    expect(dialogs).toContain('shouldIgnoreDismiss');
  });
});

describe('agent-card install log title', () => {
  it('keeps running titles and switches on done / failed (not completed)', () => {
    expect(agentTaskLogTitleKey('install', 'running')).toBe('agents.card.installing');
    expect(agentTaskLogTitleKey('upgrade', 'running')).toBe('agents.card.upgrading');
    expect(agentTaskLogTitleKey('oneclick', 'running')).toBe('agents.card.oneclickProgress');
    expect(agentTaskLogTitleKey('install', 'done')).toBe('agents.lifecycle.installComplete');
    expect(agentTaskLogTitleKey('upgrade', 'done')).toBe('agents.lifecycle.upgradeDone');
    expect(agentTaskLogTitleKey('oneclick', 'done')).toBe('agents.lifecycle.oneclickDone');
    expect(agentTaskLogTitleKey('install', 'failed')).toBe('agents.lifecycle.installFailed');
    expect(agentTaskLogTitleKey('upgrade', 'failed')).toBe('agents.lifecycle.upgradeFailed');
    expect(agentTaskLogTitleKey('oneclick', 'failed')).toBe('agents.lifecycle.oneclickFailed');
    expect(agentTaskLogTitleKey('install', 'guided')).toBe('agents.lifecycle.setupGuide');
    expect(agentTaskLogTitleKey('upgrade', 'guided')).toBe('agents.lifecycle.setupGuide');
    expect(zh.agents.lifecycle.installComplete).toBe('安装完成');
    expect(zh.agents.lifecycle.upgradeDone).toBe('升级完成');
    expect(zh.agents.lifecycle.oneclickDone).toBe('已完成');
    expect(zh.agents.lifecycle.installFailed).toBe('安装失败');
    expect(zh.agents.lifecycle.upgradeFailed).toBe('升级失败');
    expect(zh.agents.lifecycle.oneclickFailed).toBe('未完成');
    expect(zh.agents.lifecycle.setupGuide).toBe('已打开官网安装页');
    expect(zh.agents.lifecycle.setupGuide).not.toContain('失败');
    expect(zh.agents.card.needsNode22).toBe('需要 Node 22');
  });

  it('wires the card header to status, not action-only 安装中', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(card).toContain('agentTaskLogTitleKey(task.action, task.status)');
    expect(card).not.toMatch(
      /task\.action === 'install'\s*\? t\('agents\.card\.installing'\)/,
    );
  });

  it('shows 需要 Node 22 instead of 更新未知 when the note is Node too old', () => {
    expect(isNodeTooOldUpdateNote('Node too old: Pi requires Node.js >= 22')).toBe(true);
    expect(isNodeTooOldUpdateNote('已安装但未读到本机版本号')).toBe(false);
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(card).toContain('needsNode22');
    expect(card).toContain('isNodeTooOldUpdateNote');
    expect(zh.agents.card.needsNode22).toBe('需要 Node 22');
  });
});

describe('extra copy labels', () => {
  it('maps known kinds and leaves npm/native as channel ids', () => {
    expect(extraCopyKindLabelKey('ide')).toBe('agents.card.extraCopyIde');
    expect(extraCopyKindLabelKey('desktop')).toBe('agents.card.extraCopyDesktop');
    expect(extraCopyKindLabelKey('leftover-agenthub')).toBe(
      'agents.card.extraCopyLeftover',
    );
    expect(extraCopyKindLabelKey('npm')).toBeUndefined();
    expect(extraCopyKindLabelKey('native')).toBeUndefined();
    expect(extraCopyKindLabel('npm', (key) => key)).toBe('npm');
    expect(extraCopyKindLabel('ide', (key) => key)).toBe('agents.card.extraCopyIde');
  });

  it('lists unique versions on the card and points extra copies to details', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(card).toContain('uniqueInstallVersions');
    expect(card).toContain('listAgentInstalls');
    expect(card).toContain("t('agents.card.seeDetails')");
    expect(card).not.toContain('CopyVersionButton');
    expect(card).not.toContain('copyVersion');
    expect(card).not.toContain('extraCopyKindLabel');
    expect(card).not.toContain('agents.card.installSource');
    expect(card).not.toContain('agents.card.updateChannel');
    expect(card).not.toContain('agents.card.uninstallMethod');
    expect(card).not.toContain('agents.card.installLocation');
    expect(card).not.toContain('copyInstallPath');
    expect(card).not.toContain('<Hint label={inst.location}');
    expect(card).not.toContain('specialChannelUpdateTargets');
    expect(card).not.toContain('updateViaDesktop');
    expect(zh.agents.card.seeDetails).toBe('多个版本，点开看详情');
    expect(zh.agents.card.updateViaDesktop).toBe('请到桌面应用更新');
    expect(zh.agents.card.updateViaIde).toBe('请到 IDE 插件更新');
    expect(zh.agents.dialog.installAlongsideDesc).toContain('不会被替换');
    expect(zh.agents.card.extraCopyDesktop).toBe('桌面应用');
  });

  it('dedupes install versions for the compact card', () => {
    expect(
      uniqueInstallVersions([
        { version: '0.50.0' },
        { version: 'v0.50.0' },
        { version: '0.49.0' },
        { version: null },
      ]),
    ).toEqual(['v0.50.0', 'v0.49.0']);
  });

  it('compares extra copies against the shared remote latest, skipping leftover', () => {
    expect(extraCopyUpdateHint('npm', '1.0.0', '1.2.0')).toBe('update_available');
    expect(extraCopyUpdateHint('native', '2.1.50', '2.1.50')).toBe('up_to_date');
    expect(extraCopyUpdateHint('ide', '0.149.0-alpha.4.3', '0.149.1')).toBe(
      'update_available',
    );
    expect(extraCopyUpdateHint('leftover-agenthub', '0.1.0', '1.0.0')).toBeUndefined();
    expect(extraCopyUpdateHint('npm', undefined, '1.2.0')).toBeUndefined();
  });

  it('objectifies each copy: source, location, update, uninstall', () => {
    expect(installLifecycle('npm')).toEqual({
      source: 'npm',
      updateVia: 'in_app',
      uninstallVia: 'in_app',
    });
    expect(installLifecycle('native', 'workbuddy')).toEqual({
      source: 'native',
      updateVia: 'official',
      uninstallVia: 'in_app',
    });
    expect(installLifecycle('ide', 'claude')).toEqual({
      source: 'ide',
      updateVia: 'ide',
      uninstallVia: 'ide',
    });
    expect(
      listAgentInstalls({
        agentId: 'workbuddy',
        installed: true,
        channel: 'native',
        binPath: '/opt/workbuddy',
        version: '1.0.0',
        updateVia: 'official',
        uninstallVia: 'in_app',
      }),
    ).toEqual([
      {
        source: 'native',
        location: '/opt/workbuddy',
        version: '1.0.0',
        updateVia: 'official',
        uninstallVia: 'in_app',
        spawn: true,
        kind: 'native',
      },
    ]);
    const rows = listAgentInstalls({
      agentId: 'codex',
      installed: true,
      channel: 'desktop',
      binPath: 'C:\\Store\\codex.exe',
      version: '0.50.0',
      extraCopies: [
        {
          path: 'C:\\Users\\x\\.vscode\\extensions\\openai.chatgpt\\codex.exe',
          kind: 'ide',
          version: '0.49.0',
        },
      ],
    });
    expect(rows).toEqual([
      {
        source: 'desktop',
        location: 'C:\\Store\\codex.exe',
        version: '0.50.0',
        updateVia: 'desktop',
        uninstallVia: 'desktop',
        spawn: true,
        kind: 'desktop',
      },
      {
        source: 'ide',
        location: 'C:\\Users\\x\\.vscode\\extensions\\openai.chatgpt\\codex.exe',
        version: '0.49.0',
        updateVia: 'ide',
        uninstallVia: 'ide',
        spawn: false,
        kind: 'ide',
      },
    ]);
    expect(isInAppUpgradeChannel('npm')).toBe(true);
    expect(isInAppUpgradeChannel('desktop')).toBe(false);
    expect(isSpecialInstallChannel('desktop')).toBe(true);
    expect(
      canUninstallProgramInApp({
        agentId: 'codex',
        installed: true,
        channel: 'npm',
        binPath: '/npm/codex',
      }),
    ).toBe(true);
    expect(
      canUninstallProgramInApp({
        agentId: 'codex',
        installed: true,
        channel: 'ide',
        binPath: '/ide/codex',
      }),
    ).toBe(false);
    expect(
      canInstallAlongsideSpecial({
        agentId: 'codex',
        installed: true,
        channel: 'desktop',
        binPath: '/store/codex',
      }),
    ).toBe(true);
    expect(
      canInstallAlongsideSpecial({
        agentId: 'codex',
        installed: true,
        channel: 'npm',
        binPath: '/npm/codex',
      }),
    ).toBe(false);
  });

  it('hints after the agent name for special copies that cannot be upgraded here', () => {
    expect(
      specialChannelUpdateTargets({
        agentId: 'codex',
        installed: true,
        channel: 'desktop',
        binPath: '/store/codex',
        extraCopies: [],
        latestVersion: '0.51.0',
        update: { agentId: 'codex', state: 'update_available', latestVersion: '0.51.0' },
      }),
    ).toEqual([{ kind: 'desktop', outdated: true }]);
    expect(
      specialChannelUpdateTargets({
        agentId: 'codex',
        installed: true,
        channel: 'npm',
        binPath: '/npm/codex',
        extraCopies: [
          {
            path: '/ide/codex',
            kind: 'ide',
            version: '0.49.0',
            channel: null,
          },
        ],
        latestVersion: '0.50.0',
        update: { agentId: 'codex', state: 'up_to_date', latestVersion: '0.50.0' },
      }),
    ).toEqual([{ kind: 'ide', outdated: true }]);
    expect(
      specialChannelUpdateTargets({
        agentId: 'codex',
        installed: true,
        channel: 'desktop',
        binPath: '/store/codex',
        extraCopies: [],
        latestVersion: '0.50.0',
        update: { agentId: 'codex', state: 'up_to_date', latestVersion: '0.50.0' },
      }),
    ).toEqual([]);
  });
});

describe('agent-card install confirm', () => {
  it('opens confirm instead of starting install immediately', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    const dialogs = readFileSync(path.join(dir, 'AgentCardDialogs.tsx'), 'utf8');
    expect(card).toContain("openConfirm('install')");
    expect(card).not.toMatch(/onClick=\{\(\) => startAgentInstall\(selectedChannel\)\}/);
    expect(dialogs).toContain('agents.dialog.confirmInstall');
    expect(dialogs).toContain('onConfirmInstall');
    expect(dialogs).toContain('installAlongsideDesc');
    expect(dialogs).toContain('uninstallConfigKeepsApp');
  });

  it('makes retry the primary button after a real failure, not after setup-guide', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(installRetryButtonVariant('failed')).toBe('default');
    expect(installRetryButtonVariant('guided')).toBe('secondary');
    expect(installRetryButtonVariant('done')).toBe('secondary');
    expect(installRetryButtonVariant(undefined)).toBe('secondary');
    expect(card).toContain('installFailed');
    expect(card).toContain('installRetryButtonVariant(task?.status)');
    expect(card).toContain('variant="default"');
    expect(card).toContain('agents.card.retry');
    expect(card).toContain('retryAction');
    expect(card).toContain('{task.diagnosis ? (');
    expect(card.lastIndexOf('{task.diagnosis ? (')).toBeLessThan(
      card.lastIndexOf('<InlineTerminal'),
    );
  });
});

