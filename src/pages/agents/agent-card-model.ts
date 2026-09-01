import type { TerminalStatus } from '@/components/shared/InlineTerminal';
import type { InstallChannelMeta } from '@/config/agents';
import type { MessageKey } from '@/lib/i18n';
import type { AgentStatus } from '@/lib/types';
import { installLifecycle } from '@/lib/backend/contracts/install-lifecycle';

export { installLifecycle };

export type AgentCardTaskAction = 'install' | 'upgrade' | 'oneclick';
export type AgentCardTaskStatus = TerminalStatus;

/** Inline install-log header: branch on status (`done` / `failed` / `guided`), not action alone. */
export function agentTaskLogTitleKey(
  action: AgentCardTaskAction,
  status: AgentCardTaskStatus,
): MessageKey {
  if (status === 'guided') return 'agents.lifecycle.setupGuide';
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

/** Real failures use the page CTA. Opening an official setup page is not a failure. */
export function installRetryButtonVariant(
  status: AgentCardTaskStatus | undefined,
): 'default' | 'secondary' {
  return status === 'failed' ? 'default' : 'secondary';
}

/** Update chip: Node-too-old (Pi) is not a generic "update unknown". */
export function isNodeTooOldUpdateNote(note?: string | null): boolean {
  return !!note && /node too old/i.test(note);
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

/** Localized extra-copy / install-channel kind; raw `native` is never product copy. */
export function extraCopyKindLabelKey(kind: string): MessageKey | undefined {
  switch (kind) {
    case 'ide':
      return 'agents.card.extraCopyIde';
    case 'desktop':
      return 'agents.card.extraCopyDesktop';
    case 'leftover-agenthub':
      return 'agents.card.extraCopyLeftover';
    case 'native':
      return 'agents.card.channelOfficial';
    case 'npm':
      return 'agents.card.channelNpm';
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

export type AgentUpgradeKind = 'in_app' | 'open_setup' | 'hint_only';

export type AgentUpgradeControl = {
  show: boolean;
  /** Not an in-app upgrade — gray the button. */
  muted: boolean;
  kind: AgentUpgradeKind;
};

/** IDE / desktop / official / unsupported: gray button + hint, or open the setup page. */
export function agentUpgradeControl(input: {
  installed: boolean;
  updateVia?: string | null;
  updateState?: string;
  setupUrl?: string | null;
}): AgentUpgradeControl {
  if (!input.installed) {
    return { show: false, muted: false, kind: 'in_app' };
  }
  const via = asUpdateVia(input.updateVia);
  const unsupported = input.updateState === 'unsupported';
  if (via === 'in_app' && !unsupported) {
    return { show: true, muted: false, kind: 'in_app' };
  }
  const show =
    unsupported || via === 'official' || via === 'ide' || via === 'desktop';
  if (!show) {
    return { show: false, muted: false, kind: 'hint_only' };
  }
  const url = input.setupUrl?.trim();
  const hasUrl = Boolean(url && /^https:\/\//i.test(url));
  return {
    show: true,
    muted: true,
    kind: hasUrl ? 'open_setup' : 'hint_only',
  };
}

export function agentUpgradeHint(
  control: Pick<AgentUpgradeControl, 'kind' | 'muted'>,
  input: {
    updateVia?: string | null;
    note?: string | null;
    t: (key: MessageKey, params?: Record<string, string>) => string;
  },
): string {
  const via = asUpdateVia(input.updateVia);
  const where =
    via && via !== 'in_app' && via !== 'none'
      ? updateViaLabel(via, input.t)
      : input.t('agents.card.unsupportedUpdate');
  const hint = input.note?.trim() || where;
  if (control.kind === 'open_setup') {
    return input.t('agents.update.clickOfficial', { note: hint });
  }
  return hint;
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
  agent: Pick<
    AgentStatus,
    'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies' | 'updateVia' | 'uninstallVia'
  >,
): AgentInstall[] {
  const out: AgentInstall[] = [];
  const spawnPath = agent.binPath?.trim();
  if (agent.installed && spawnPath) {
    const kind = isInstallSource(agent.channel) ? agent.channel : 'native';
    const spawnCopy = (agent.extraCopies ?? []).find(
      (copy) => copy.path?.trim() && sameInstallPath(spawnPath, copy.path.trim()),
    );
    out.push(
      toAgentInstall(agent.agentId, kind, spawnPath, agent.version, true, spawnCopy ?? {
        source: kind,
        updateVia: agent.updateVia,
        uninstallVia: agent.uninstallVia,
      }),
    );
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

export type AgentLaunchTargets = {
  cliPath?: string;
  appPath?: string;
};

function launchKindForInstall(
  agentId: string,
  row: Pick<AgentInstall, 'source'>,
): 'cli' | 'app' | null {
  if (row.source === 'desktop') return 'app';
  if (row.source === 'npm') return 'cli';
  if (row.source === 'native') {
    // WorkBuddy / ZCode native Setup is the Electron app, not a CLI.
    if (installLifecycle('native', agentId).updateVia === 'official') return 'app';
    return 'cli';
  }
  return null;
}

/** Outer card: show 启动 CLI / 启动 App only when that program exists. */
export function agentLaunchTargets(
  agent: Pick<AgentStatus, 'agentId' | 'installed' | 'binPath' | 'channel' | 'version' | 'extraCopies'>,
): AgentLaunchTargets {
  const out: AgentLaunchTargets = {};
  for (const row of listAgentInstalls(agent)) {
    const location = row.location.trim();
    if (!location) continue;
    const kind = launchKindForInstall(agent.agentId, row);
    if (kind === 'cli' && !out.cliPath) out.cliPath = location;
    if (kind === 'app' && !out.appPath) out.appPath = location;
  }
  return out;
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

export function isLeftoverInstallSource(source?: string | null): boolean {
  return source === 'leftover-agenthub';
}

export function programInstalls<T extends { source: string }>(installs: readonly T[]): T[] {
  return installs.filter((row) => !isLeftoverInstallSource(row.source));
}

export type AgentListDetailsHint = {
  key: MessageKey;
  params?: { count: number };
};

/**
 * List hint must match detail locations. Leftover copies are leftover, not versions.
 */
export function agentListDetailsHint(
  installs: ReadonlyArray<{ source: string; version?: string | null }>,
): AgentListDetailsHint | null {
  if (installs.length <= 1) return null;
  const leftoverCount = installs.filter((row) => isLeftoverInstallSource(row.source)).length;
  const extraCount = installs.length - 1;
  const programVersions = uniqueInstallVersions(programInstalls(installs));
  const allVersions = uniqueInstallVersions(installs);
  if (leftoverCount > 0 && leftoverCount === extraCount && programVersions.length <= 1) {
    return { key: 'agents.card.seeDetailsLeftover', params: { count: leftoverCount } };
  }
  if (
    leftoverCount === 0 &&
    programVersions.length > 1 &&
    allVersions.length === installs.length
  ) {
    return { key: 'agents.card.seeDetails' };
  }
  return { key: 'agents.card.seeDetailsCopies', params: { count: extraCount } };
}

/** Unique formatted versions for the compact card; order follows install list. */
export function uniqueInstallVersions(
  installs: ReadonlyArray<{ version?: string | null }>,
): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const inst of installs) {
    const v = formatAgentVersion(inst.version);
    if (!v) continue;
    const key = v.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(v);
  }
  return out;
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
