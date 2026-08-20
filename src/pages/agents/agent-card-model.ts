import type { InstallChannelMeta } from '@/config/agents';
import type { MessageKey } from '@/lib/i18n';
import { handleMenuDialogSelect } from '@/pages/connections/ticket-wallet-model';

export type AgentCardTaskAction = 'install' | 'upgrade' | 'oneclick';
export type AgentCardTaskStatus = 'running' | 'done' | 'failed';

/** Inline install-log header: branch on status (`done` / `failed`), not action alone. */
export function agentTaskLogTitleKey(
  action: AgentCardTaskAction,
  status: AgentCardTaskStatus,
): MessageKey {
  if (status === 'done') {
    if (action === 'oneclick') return 'agents.lifecycle.oneclickDone';
    if (action === 'install') return 'agents.lifecycle.installComplete';
    return 'agents.lifecycle.upgradeDone';
  }
  if (status === 'failed') {
    if (action === 'oneclick') return 'agents.lifecycle.oneclickFailed';
    if (action === 'install') return 'agents.lifecycle.installFailed';
    return 'agents.lifecycle.upgradeFailed';
  }
  if (action === 'oneclick') return 'agents.card.oneclickProgress';
  if (action === 'install') return 'agents.card.installing';
  return 'agents.card.upgrading';
}

/** Update chip: Node-too-old (Pi) is not a generic "update unknown". */
export function isNodeTooOldUpdateNote(note?: string | null): boolean {
  return !!note && /node too old/i.test(note);
}

export type AgentCardUninstallConfirmKind = 'program' | 'config';

/** Same menu→Dialog path as Connections add-menu: preventDefault, arm, open, clear. */
export function openAgentCardUninstallConfirm(
  event: { preventDefault: () => void },
  kind: AgentCardUninstallConfirmKind,
  setConfirmDialog: (kind: AgentCardUninstallConfirmKind) => void,
  ignoreRef: { current: boolean },
): void {
  handleMenuDialogSelect(event, ignoreRef, () => setConfirmDialog(kind));
}

export function resolveOfficialSetupUrl(
  updateSetupUrl: string | undefined,
  channels: InstallChannelMeta[],
): string | undefined {
  const fromProbe = updateSetupUrl?.trim();
  if (fromProbe && /^https:\/\//i.test(fromProbe)) return fromProbe;
  for (const ch of channels) {
    if (ch.id !== 'native') continue;
    const cmd = ch.command?.trim();
    if (cmd && /^https:\/\//i.test(cmd)) return cmd;
  }
  return undefined;
}

/**
 * Format CLI version for UI: strip name noise so `codex-cli 0.144.5` → `v0.144.5`
 * (avoids the broken `vcodex-cli 0.144.5` when prefixing a raw `v`).
 */
export function formatAgentVersion(raw?: string | null): string | undefined {
  if (!raw?.trim()) return undefined;
  const s = raw.trim();
  const token =
    s
      .split(/[\s()]+/)
      .map((p) => p.trim())
      .filter(Boolean)
      .find((p) => {
        const t = p.replace(/^[vV]/, '');
        return t.length > 0 && /^\d/.test(t);
      }) ?? s;
  const cleaned = token.replace(/^[vV]/, '').replace(/[,;)]+$/g, '');
  if (!cleaned || !/\d/.test(cleaned)) return s;
  return `v${cleaned}`;
}
