import type { InstallChannelMeta } from '@/config/agents';
import type { MessageKey } from '@/lib/i18n';
import type { AgentStatus } from '@/lib/types';
import { handleMenuDialogSelect } from '@/lib/menu-dialog-arm';

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
  openConfirm: (kind: AgentCardUninstallConfirmKind) => void,
  ignoreRef: { current: boolean },
): void {
  handleMenuDialogSelect(event, ignoreRef, () => openConfirm(kind));
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

export function updateViaLabel(
  via: UpdateVia,
  t: (key: MessageKey) => string,
): string {
  switch (via) {
    case 'in_app':
      return t('agents.card.viaInApp');
    case 'ide':
      return t('agents.card.updateViaIde');
    case 'desktop':
      return t('agents.card.updateViaDesktop');
    case 'official':
      return t('agents.card.needsOfficial');
    default:
      return t('agents.card.viaNone');
  }
}

export function uninstallViaLabel(
  via: UninstallVia,
  t: (key: MessageKey) => string,
): string {
  switch (via) {
    case 'in_app':
      return t('agents.card.viaInApp');
    case 'ide':
      return t('agents.card.uninstallViaIde');
    case 'desktop':
      return t('agents.card.uninstallViaDesktop');
    case 'official':
      return t('agents.card.needsOfficial');
    case 'leftover':
      return t('agents.card.viaLeftover');
    default:
      return t('agents.card.viaNone');
  }
}

/**
 * Compare one extra copy against the shared remote latest.
 * Leftover data-dir npm is not an upgrade target — skip the hint.
 * Upgrade still uses the spawn channel; this is display-only.
 */
export type ExtraCopyUpdateHint = 'update_available' | 'up_to_date' | 'unknown';

export const SPECIAL_INSTALL_CHANNELS = ['desktop', 'ide'] as const;
export type SpecialInstallChannel = (typeof SPECIAL_INSTALL_CHANNELS)[number];

export type InstallSource = 'npm' | 'native' | 'ide' | 'desktop' | 'leftover-agenthub';
export type UpdateVia = 'in_app' | 'ide' | 'desktop' | 'official' | 'none';
export type UninstallVia = 'in_app' | 'ide' | 'desktop' | 'official' | 'leftover' | 'none';

export type AgentInstall = {
  source: InstallSource;
  location: string;
  version?: string | null;
  updateVia: UpdateVia;
  uninstallVia: UninstallVia;
  spawn: boolean;
  kind: string;
};

export function isSpecialInstallChannel(
  channel?: string | null,
): channel is SpecialInstallChannel {
  return channel === 'desktop' || channel === 'ide';
}

export function isInstallSource(value?: string | null): value is InstallSource {
  return (
    value === 'npm' ||
    value === 'native' ||
    value === 'ide' ||
    value === 'desktop' ||
    value === 'leftover-agenthub'
  );
}

/** Same object for every agent copy so UI does not mix npm / IDE / Store. */
export function installLifecycle(
  kind: string,
  agentId?: string,
): Pick<AgentInstall, 'source' | 'updateVia' | 'uninstallVia'> {
  if (kind === 'npm') {
    return { source: 'npm', updateVia: 'in_app', uninstallVia: 'in_app' };
  }
  if (kind === 'native' && agentId === 'workbuddy') {
    return { source: 'native', updateVia: 'official', uninstallVia: 'in_app' };
  }
  if (kind === 'native') {
    return { source: 'native', updateVia: 'in_app', uninstallVia: 'in_app' };
  }
  if (kind === 'ide') {
    return { source: 'ide', updateVia: 'ide', uninstallVia: 'ide' };
  }
  if (kind === 'desktop') {
    return { source: 'desktop', updateVia: 'desktop', uninstallVia: 'desktop' };
  }
  if (kind === 'leftover-agenthub') {
    return { source: 'leftover-agenthub', updateVia: 'none', uninstallVia: 'leftover' };
  }
  return { source: 'native', updateVia: 'none', uninstallVia: 'none' };
}

function sameInstallPath(a: string, b: string): boolean {
  return a.replace(/\\/g, '/').toLowerCase() === b.replace(/\\/g, '/').toLowerCase();
}

function asUpdateVia(value?: string | null): UpdateVia | undefined {
  if (
    value === 'in_app' ||
    value === 'ide' ||
    value === 'desktop' ||
    value === 'official' ||
    value === 'none'
  ) {
    return value;
  }
  return undefined;
}

function asUninstallVia(value?: string | null): UninstallVia | undefined {
  if (
    value === 'in_app' ||
    value === 'ide' ||
    value === 'desktop' ||
    value === 'official' ||
    value === 'leftover' ||
    value === 'none'
  ) {
    return value;
  }
  return undefined;
}

function toAgentInstall(
  agentId: string,
  kind: string,
  location: string,
  version: string | null | undefined,
  spawn: boolean,
  copy?: {
    source?: string | null;
    updateVia?: string | null;
    uninstallVia?: string | null;
  },
): AgentInstall {
  const fallbackKind = isInstallSource(kind) ? kind : 'native';
  const life = installLifecycle(fallbackKind, agentId);
  const source = isInstallSource(copy?.source) ? copy.source : life.source;
  return {
    source,
    location,
    version,
    updateVia: asUpdateVia(copy?.updateVia) ?? life.updateVia,
    uninstallVia: asUninstallVia(copy?.uninstallVia) ?? life.uninstallVia,
    spawn,
    kind: fallbackKind,
  };
}

/** Spawn copy + extras, each with source / location / update / uninstall. */
export function listAgentInstalls(
  agent: Pick<AgentStatus, 'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies'>,
): AgentInstall[] {
  const out: AgentInstall[] = [];
  const spawnPath = agent.binPath?.trim();
  if (agent.installed && spawnPath) {
    const kind = isInstallSource(agent.channel) ? agent.channel : 'native';
    out.push(toAgentInstall(agent.agentId, kind, spawnPath, agent.version, true));
  }
  for (const copy of agent.extraCopies ?? []) {
    const location = copy.path?.trim();
    if (!location) continue;
    if (spawnPath && sameInstallPath(spawnPath, location)) continue;
    out.push(
      toAgentInstall(
        agent.agentId,
        copy.kind || copy.source || 'native',
        location,
        copy.version,
        false,
        copy,
      ),
    );
  }
  return out;
}

/** npm / native can be upgraded here; missing channel still uses native install. */
export function isInAppUpgradeChannel(channel?: string | null): boolean {
  if (!channel?.trim()) return true;
  return channel === 'npm' || channel === 'native';
}

export function spawnInstall(
  agent: Pick<AgentStatus, 'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies'>,
): AgentInstall | undefined {
  return listAgentInstalls(agent).find((row) => row.spawn);
}

/** Program uninstall only covers copies whose uninstall method is in-app. */
export function canUninstallProgramInApp(
  agent: Pick<AgentStatus, 'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies'>,
): boolean {
  return listAgentInstalls(agent).some((row) => row.uninstallVia === 'in_app');
}

/** Plugin/desktop copies stay; npm/native can still be installed beside them. */
export function canInstallAlongsideSpecial(
  agent: Pick<AgentStatus, 'installed' | 'agentId' | 'binPath' | 'channel' | 'version' | 'extraCopies'>,
): boolean {
  if (!agent.installed) return false;
  return !listAgentInstalls(agent).some(
    (row) => row.source === 'npm' || row.source === 'native',
  );
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
  agent: Pick<
    AgentStatus,
    'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies' | 'latestVersion' | 'update'
  >,
): SpecialChannelUpdateTarget[] {
  const latest = agent.update?.latestVersion ?? agent.latestVersion;
  const state = agent.update?.state;
  const outdated = new Set<SpecialInstallChannel>();
  const shown = new Set<SpecialInstallChannel>();
  for (const inst of listAgentInstalls(agent)) {
    if (!isSpecialInstallChannel(inst.source)) continue;
    const copyHint = extraCopyUpdateHint(inst.source, inst.version, latest);
    if (copyHint === 'update_available') {
      shown.add(inst.source);
      outdated.add(inst.source);
      continue;
    }
    if (
      inst.spawn &&
      state !== 'up_to_date' &&
      state !== 'checking'
    ) {
      shown.add(inst.source);
      if (state === 'update_available') outdated.add(inst.source);
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
