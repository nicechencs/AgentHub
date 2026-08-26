import type { InstallChannelMeta } from '@/config/agents';
import type { MessageKey } from '@/lib/i18n';
import type { AgentStatus } from '@/lib/types';
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

/** Localized extra-copy kind; unknown kinds (npm/native) are shown as-is. */
export function extraCopyKindLabelKey(kind: string): MessageKey | undefined {
  switch (kind) {
    case 'ide':
      return 'agents.card.extraCopyIde';
    case 'desktop':
      return 'agents.card.extraCopyDesktop';
    case 'leftover-agenthub':
      return 'agents.card.extraCopyLeftover';
    default:
      return undefined;
  }
}

export function extraCopyKindLabel(
  kind: string,
  t: (key: MessageKey) => string,
): string {
  const key = extraCopyKindLabelKey(kind);
  return key ? t(key) : kind;
}

/**
 * Compare one extra copy against the shared remote latest.
 * Leftover data-dir npm is not an upgrade target — skip the hint.
 * Upgrade still uses the spawn channel; this is display-only.
 */
export type ExtraCopyUpdateHint = 'update_available' | 'up_to_date' | 'unknown';

export const SPECIAL_INSTALL_CHANNELS = ['desktop', 'ide'] as const;
export type SpecialInstallChannel = (typeof SPECIAL_INSTALL_CHANNELS)[number];

export function isSpecialInstallChannel(
  channel?: string | null,
): channel is SpecialInstallChannel {
  return channel === 'desktop' || channel === 'ide';
}

/** npm / native can be upgraded here; missing channel still uses native install. */
export function isInAppUpgradeChannel(channel?: string | null): boolean {
  if (!channel?.trim()) return true;
  return channel === 'npm' || channel === 'native';
}

export type SpecialChannelUpdateTarget = {
  kind: SpecialInstallChannel;
  outdated: boolean;
};

/**
 * Special copies AgentHub cannot upgrade. Hint after the agent name so the
 * user goes to the desktop app or IDE extension.
 * Skip when that copy is already current.
 */
export function specialChannelUpdateTargets(
  agent: Pick<AgentStatus, 'channel' | 'extraCopies' | 'latestVersion' | 'update'>,
): SpecialChannelUpdateTarget[] {
  const latest = agent.update?.latestVersion ?? agent.latestVersion;
  const state = agent.update?.state;
  const outdated = new Set<SpecialInstallChannel>();
  const shown = new Set<SpecialInstallChannel>();
  if (
    isSpecialInstallChannel(agent.channel) &&
    state !== 'up_to_date' &&
    state !== 'checking'
  ) {
    shown.add(agent.channel);
    if (state === 'update_available') outdated.add(agent.channel);
  }
  for (const copy of agent.extraCopies ?? []) {
    if (!isSpecialInstallChannel(copy.kind)) continue;
    if (extraCopyUpdateHint(copy.kind, copy.version, latest) === 'update_available') {
      shown.add(copy.kind);
      outdated.add(copy.kind);
    }
  }
  return SPECIAL_INSTALL_CHANNELS.filter((kind) => shown.has(kind)).map((kind) => ({
    kind,
    outdated: outdated.has(kind),
  }));
}

export function extraCopyUpdateHint(
  kind: string,
  copyVersion: string | null | undefined,
  latestVersion: string | null | undefined,
): ExtraCopyUpdateHint | undefined {
  if (kind === 'leftover-agenthub') return undefined;
  const local = versionCore(copyVersion);
  const remote = versionCore(latestVersion);
  if (!local || !remote) return copyVersion?.trim() && latestVersion?.trim()
    ? 'unknown'
    : undefined;
  const cmp = compareVersionCores(local, remote);
  return cmp < 0 ? 'update_available' : 'up_to_date';
}

/** Leading x.y.z integers; null if incomparable. Negative if a < b. */
export function compareLooseVersions(a: string, b: string): number | null {
  const left = versionCore(a);
  const right = versionCore(b);
  if (!left || !right) return null;
  return compareVersionCores(left, right);
}

function versionCore(raw?: string | null): [number, number, number] | null {
  const formatted = formatAgentVersion(raw);
  if (!formatted) return null;
  const token = formatted.replace(/^[vV]/, '');
  const m = token.match(/^(\d+)(?:\.(\d+))?(?:\.(\d+))?/);
  if (!m) return null;
  return [Number(m[1]), Number(m[2] ?? 0), Number(m[3] ?? 0)];
}

function compareVersionCores(
  a: [number, number, number],
  b: [number, number, number],
): number {
  for (let i = 0; i < 3; i++) {
    const d = a[i]! - b[i]!;
    if (d !== 0) return d < 0 ? -1 : 1;
  }
  return 0;
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
