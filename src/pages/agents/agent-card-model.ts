import type { InstallChannelMeta } from '@/config/agents';

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
