import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { shouldIgnoreMenuDialogDismiss } from '@/pages/connections/ticket-wallet-model';
import { zh } from '@/lib/i18n/locales/zh';
import {
  agentTaskLogTitleKey,
  isNodeTooOldUpdateNote,
  openAgentCardUninstallConfirm,
} from './agent-card-model';

const dir = path.dirname(fileURLToPath(import.meta.url));

afterEach(() => {
  vi.useRealTimers();
});

describe('openAgentCardUninstallConfirm', () => {
  it('swallows select, opens program/config, and arms the leftover dismiss', () => {
    vi.useFakeTimers();
    const event = { preventDefault: vi.fn() };
    const setConfirmDialog = vi.fn();
    const ignoreRef = { current: false };

    openAgentCardUninstallConfirm(event, 'program', setConfirmDialog, ignoreRef);

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(setConfirmDialog).toHaveBeenCalledOnce();
    expect(setConfirmDialog).toHaveBeenCalledWith('program');
    expect(ignoreRef.current).toBe(true);
    expect(shouldIgnoreMenuDialogDismiss(ignoreRef.current, false)).toBe(true);
    vi.advanceTimersByTime(100);
    expect(ignoreRef.current).toBe(false);
  });

  it('opens the delete-config confirm without touching navigate/copy/install', () => {
    vi.useFakeTimers();
    const event = { preventDefault: vi.fn() };
    const setConfirmDialog = vi.fn();
    const ignoreRef = { current: false };

    openAgentCardUninstallConfirm(event, 'config', setConfirmDialog, ignoreRef);

    expect(setConfirmDialog).toHaveBeenCalledWith('config');
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

describe('agent-card install confirm', () => {
  it('opens confirm instead of starting install immediately', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    const dialogs = readFileSync(path.join(dir, 'AgentCardDialogs.tsx'), 'utf8');
    expect(card).toContain("setConfirmDialog('install')");
    expect(card).not.toMatch(/onClick=\{\(\) => startAgentInstall\(selectedChannel\)\}/);
    expect(dialogs).toContain('agents.dialog.confirmInstall');
    expect(dialogs).toContain('onConfirmInstall');
  });

  it('makes retry the primary button after a failed task', () => {
    const card = readFileSync(path.join(dir, 'agent-card.tsx'), 'utf8');
    expect(card).toContain('installFailed');
    expect(card).toContain("variant={installFailed ? 'default' : 'secondary'}");
    expect(card).toContain('agents.card.retry');
    expect(card).toContain('retryAction');
  });
});

