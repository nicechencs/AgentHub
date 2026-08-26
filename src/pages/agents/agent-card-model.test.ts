import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { shouldIgnoreMenuDialogDismiss } from '@/pages/connections/ticket-wallet-model';
import { zh } from '@/lib/i18n/locales/zh';
import {
  agentTaskLogTitleKey,
  extraCopyKindLabel,
  extraCopyKindLabelKey,
  extraCopyUpdateHint,
  canInstallAlongsideSpecial,
  canUninstallProgramInApp,
  installLifecycle,
  isInAppUpgradeChannel,
  listAgentInstalls,
  isNodeTooOldUpdateNote,
  isSpecialInstallChannel,
  openAgentCardUninstallConfirm,
  specialChannelUpdateTargets,
} from './agent-card-model';

const dir = path.dirname(fileURLToPath(import.meta.url));

afterEach(() => {
  vi.useRealTimers();
});

describe('openAgentCardUninstallConfirm', () => {
  it('swallows select, opens program/config, and arms the leftover dismiss', () => {
    vi.useFakeTimers();
    const event = { preventDefault: vi.fn() };
    const openConfirm = vi.fn();
    const ignoreRef = { current: false };

    openAgentCardUninstallConfirm(event, 'program', openConfirm, ignoreRef);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(openConfirm).toHaveBeenCalledOnce();
    expect(openConfirm).toHaveBeenCalledWith('program');
    expect(ignoreRef.current).toBe(true);
    expect(shouldIgnoreMenuDialogDismiss(ignoreRef.current, false)).toBe(true);
    vi.advanceTimersByTime(100);
    expect(ignoreRef.current).toBe(false);
  });

  it('opens the delete-config confirm without touching navigate/copy/install', () => {
    vi.useFakeTimers();
    const event = { preventDefault: vi.fn() };
    const openConfirm = vi.fn();
    const ignoreRef = { current: false };

    openAgentCardUninstallConfirm(event, 'config', openConfirm, ignoreRef);

    expect(openConfirm).toHaveBeenCalledWith('config');
    expect(event.preventDefault).toHaveBeenCalledOnce();
  });
});

describe('agent-card menu wiring', () => {
  it('only the uninstall items open a Dialog through the menu helper', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    const dialogs = readFileSync(path.join(dir, 'AgentCardDialogs.tsx'), 'utf8');

    expect(card).toContain('openAgentCardUninstallConfirm');
    expect(card).toContain("onSelect={(event) => openUninstallConfirm(event, 'program')}");
    expect(card).toContain("onSelect={(event) => openUninstallConfirm(event, 'config')}");
    expect(card).toContain('canUninstallProgramInApp');
    expect(card).toContain('canInstallAlongsideSpecial');
    expect(card).toContain('onCloseAutoFocus={(event) => event.preventDefault()}');
    expect(card).toContain('shouldIgnoreMenuDialogDismiss');
    expect(card).not.toMatch(/onSelect=\{\(\) => setConfirmDialog\('program'\)\}/);
    expect(card).not.toMatch(/onSelect=\{\(\) => setConfirmDialog\('config'\)\}/);
    expect(card).toMatch(/onSelect=\{\(\) => \{\s*void openBinDir\(\);/);
    expect(card).toContain('onSelect={startOneClickEnvOnly}');
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
    expect(zh.agents.lifecycle.installComplete).toBe('安装完成');
    expect(zh.agents.lifecycle.upgradeDone).toBe('升级完成');
    expect(zh.agents.lifecycle.oneclickDone).toBe('已完成');
    expect(zh.agents.lifecycle.installFailed).toBe('安装失败');
    expect(zh.agents.lifecycle.upgradeFailed).toBe('升级失败');
    expect(zh.agents.lifecycle.oneclickFailed).toBe('未完成');
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

  it('copies the version, not the install path, and keeps copy rows to one line', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(card).toContain('copyVersion');
    expect(card).toContain('CopyVersionButton');
    expect(card).toContain('agents.card.copyVersion');
    expect(card).toContain('extraCopyKindLabel');
    expect(card).toContain('listAgentInstalls');
    expect(card).not.toContain('agents.card.installSource');
    expect(card).not.toContain('agents.card.updateChannel');
    expect(card).not.toContain('agents.card.uninstallMethod');
    expect(card).not.toContain('agents.card.installLocation');
    expect(card).not.toContain('copyInstallPath');
    expect(card).toContain('<Hint label={inst.location}');
    expect(card).not.toMatch(/<p[^>]*\stitle=\{inst\.location\}/);
    expect(card).toContain('specialChannelUpdateTargets');
    expect(card).toContain('updateViaDesktop');
    expect(zh.agents.card.copyVersion).toBe('复制版本');
    expect(zh.agents.card.updateViaDesktop).toBe('请到桌面应用更新');
    expect(zh.agents.card.updateViaIde).toBe('请到 IDE 插件更新');
    expect(zh.agents.card.installAlongsideHint).toBe('也可再装其他渠道');
    expect(zh.agents.dialog.installAlongsideDesc).toContain('不会被替换');
    expect(zh.agents.card.extraCopyDesktop).toBe('桌面应用');
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

  it('keeps retry as a secondary card action after a failed task', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(card).toContain('installFailed');
    expect(card).not.toContain("variant={installFailed ? 'default' : 'secondary'}");
    expect(card).toContain('variant="secondary"');
    expect(card).toContain('agents.card.retry');
    expect(card).toContain('retryAction');
  });
});

