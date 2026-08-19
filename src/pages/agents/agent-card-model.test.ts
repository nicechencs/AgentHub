import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { shouldIgnoreMenuDialogDismiss } from '@/pages/connections/ticket-wallet-model';
import { openAgentCardUninstallConfirm } from './agent-card-model';

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

