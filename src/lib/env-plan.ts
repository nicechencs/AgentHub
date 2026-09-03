/**
 * Pure env install planning (no I/O). Shared by tauri + mock env ports.
 */
import { RUNTIME_MAP } from '@/config/runtimes';
import type { InstallOutcome } from '@/lib/backend/contracts/install-types';
import {
  detectHostPlatform,
  getRuntimeInstallChannel,
  supportsRuntimeAutoInstall,
  type HostPlatform,
  type RuntimeInstallChannel,
} from '@/lib/platform-detect';
import type { RuntimeDetect, RuntimeId } from '@/lib/types';

export type { HostPlatform, RuntimeInstallChannel } from '@/lib/platform-detect';

export interface AutoInstallPlan {
  targets: RuntimeId[];
  skipped: RuntimeId[];
  summary: string;
}

/**
 * Select the package-manager channel once for all runtime installs in a plan.
 * Keeping this beside plan resolution prevents individual UI surfaces from
 * silently falling back to Windows-only winget on macOS.
 */
export function runtimeChannelForPlan(
  platform: HostPlatform = detectHostPlatform(),
): RuntimeInstallChannel {
  return getRuntimeInstallChannel(platform);
}

function blocksAutoInstall(value: string): boolean {
  const hay = value.toLowerCase();
  return (
    hay.includes('https://brew.sh') ||
    hay.includes('homebrew/install') ||
    hay.includes('未找到 homebrew') ||
    hay.includes('未找到 winget')
  );
}

function hasInstallerChannel(
  runtimes: RuntimeDetect[],
  id: RuntimeId,
  platform: HostPlatform,
): boolean {
  const channel = getRuntimeInstallChannel(platform);
  if (channel !== 'brew' && channel !== 'winget') return false;
  const row = runtimes.find((item) => item.id === id);
  if (!row || row.remediations.length === 0) return true;
  if (row.remediations.some((item) => blocksAutoInstall(item.value))) {
    // macOS can still install Node via the official pkg when Homebrew is missing.
    return platform === 'macos' && (id === 'nodejs' || id === 'npm');
  }
  return row.remediations.some((item) => item.kind === channel);
}

export function resolveAutoInstallPlan(
  runtimes: RuntimeDetect[],
  onlyIds?: RuntimeId[],
  platform: HostPlatform = detectHostPlatform(),
  includeReady = false,
): AutoInstallPlan {
  const filter = onlyIds ? new Set(onlyIds) : null;
  const issues = runtimes.filter(
    (r) => (includeReady || r.status !== 'ok') && (!filter || filter.has(r.id)),
  );
  const issueIds = new Set(issues.map((i) => i.id));
  const targets: RuntimeId[] = [];
  const skipped: RuntimeId[] = [];
  const canAuto = supportsRuntimeAutoInstall(platform);

  if (issueIds.has('nodejs') || issueIds.has('npm')) {
    if (
      canAuto &&
      RUNTIME_MAP.nodejs.canAutoInstall &&
      hasInstallerChannel(runtimes, 'nodejs', platform)
    ) {
      targets.push('nodejs');
    } else {
      if (issueIds.has('nodejs')) skipped.push('nodejs');
      if (issueIds.has('npm')) skipped.push('npm');
    }
  }

  for (const id of issueIds) {
    if (id === 'nodejs' || id === 'npm') continue;
    if (
      canAuto &&
      RUNTIME_MAP[id].canAutoInstall &&
      hasInstallerChannel(runtimes, id, platform)
    ) {
      targets.push(id);
    } else skipped.push(id);
  }

  const summaryParts = targets.map((id) => {
    if (id === 'nodejs' && (issueIds.has('npm') || issueIds.has('nodejs'))) {
      return 'Node.js(含 npm)';
    }
    return RUNTIME_MAP[id].name;
  });

  return {
    targets,
    skipped,
    summary: summaryParts.length ? summaryParts.join('、') : '无',
  };
}

interface EnvNotReadyDetails {
  hint?: string | null;
  remediations?: Array<{
    command?: string | null;
    url?: string | null;
    text?: string | null;
  }>;
}

function asEnvNotReadyDetails(details: unknown): EnvNotReadyDetails | null {
  if (!details || typeof details !== 'object') return null;
  return details as EnvNotReadyDetails;
}

/** Turn a failed env install into copyable steps, without internal log prefixes. */
export function formatRuntimeInstallFailureLines(outcome: InstallOutcome): string[] {
  const lines: string[] = [];
  const seen = new Set<string>();
  const push = (raw?: string | null) => {
    const text = raw?.trim();
    if (!text || seen.has(text)) return;
    seen.add(text);
    lines.push(text);
  };

  if (outcome.code === 'env.not_ready') {
    push(outcome.message);
    const details = asEnvNotReadyDetails(outcome.details);
    if (details?.hint) push(details.hint);
    for (const rem of details?.remediations ?? []) {
      if (rem.text) push(rem.text);
      if (rem.command) push(rem.command);
      if (rem.url) push(rem.url);
    }
    if (lines.length > 0) return lines;
  }

  for (const line of outcome.logs ?? []) push(line);
  if (lines.length === 0) push(outcome.message);
  return lines;
}
