/**
 * Agent UI meta façade.
 *
 * Product agent set (id / name / channels / capabilities) comes from the
 * runtime Agent Catalog (`applyAgentCatalog`). This module only keeps pure
 * display tokens (letter / brand color) for known agents; unknown keys get
 * neutral fallback styling.
 */
import type { AgentCapabilities } from '@/lib/capability';
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import { mapCatalogCapabilities } from '@/lib/backend/contracts/agent-catalog-types';
import type { AgentId, RuntimeId } from '@/lib/types';
import { agentCssVar, type TokenAgentId } from '@/styles/tokens';

export interface InstallChannelMeta {
  id: string;
  label: string;
  command: string;
  /** 安装该渠道前必须就绪的共享 Runtime */
  requires: RuntimeId[];
}

export interface AgentMeta {
  id: AgentId;
  name: string;
  /** 品牌色 CSS 变量；未知 agent 为 muted fallback */
  color: string;
  /** logo 用字母圆标代替 */
  letter: string;
  /**
   * Install channels from backend catalog.
   * Empty until `applyAgentCatalog` / hydrate runs.
   */
  installChannels: InstallChannelMeta[];
  /** Catalog-declared capabilities (optional; doctor status may override). */
  capabilities?: AgentCapabilities;
}

/** Pure display decoration for known agents — not the product set. */
export const AGENT_DISPLAY: Record<
  string,
  { letter: string; colorKey: TokenAgentId }
> = {
  claude: { letter: 'C', colorKey: 'claude' },
  codex: { letter: 'X', colorKey: 'codex' },
  kimi: { letter: 'K', colorKey: 'kimi' },
  grok: { letter: 'G', colorKey: 'grok' },
  pi: { letter: 'P', colorKey: 'pi' },
  workbuddy: { letter: 'W', colorKey: 'workbuddy' },
  cursor: { letter: 'R', colorKey: 'cursor' },
};

const FALLBACK_COLOR = 'var(--text-muted)';

function letterFor(key: string, displayName: string): string {
  const known = AGENT_DISPLAY[key];
  if (known) return known.letter;
  const ch = (displayName || key).trim().charAt(0);
  return ch ? ch.toUpperCase() : '?';
}

function colorFor(key: string): string {
  const known = AGENT_DISPLAY[key];
  return known ? agentCssVar(known.colorKey) : FALLBACK_COLOR;
}

/** Build UI meta from a catalog entry (name/channels/caps from backend). */
export function agentMetaFromCatalogEntry(entry: AgentCatalogEntryDto): AgentMeta {
  return {
    id: entry.key,
    name: entry.displayName,
    color: colorFor(entry.key),
    letter: letterFor(entry.key, entry.displayName),
    installChannels: entry.installChannels.map((ch) => ({
      id: ch.id,
      label: ch.label,
      command: ch.command,
      requires: ch.requires,
    })),
    capabilities: mapCatalogCapabilities(entry.capabilities),
  };
}

/**
 * Runtime agent list — filled only by {@link applyAgentCatalog}.
 * Not a static closed set; starts empty until catalog load/seed.
 */
export const AGENTS: AgentMeta[] = [];

/** Lookup map; keys cleared/rebuilt with {@link applyAgentCatalog}. */
export const AGENT_MAP: Record<string, AgentMeta> = Object.create(null) as Record<
  string,
  AgentMeta
>;

/** Convenience id list; mirrors AGENTS order. */
export let AGENT_IDS: AgentId[] = [];

/**
 * Replace product agent set from backend catalog entries.
 * Call sites that need the full list must run after this (boot / mock seed).
 */
export function applyAgentCatalog(entries: AgentCatalogEntryDto[]): void {
  AGENTS.length = 0;
  for (const k of Object.keys(AGENT_MAP)) {
    delete AGENT_MAP[k];
  }
  for (const entry of entries) {
    const meta = agentMetaFromCatalogEntry(entry);
    AGENTS.push(meta);
    AGENT_MAP[meta.id] = meta;
  }
  AGENT_IDS = AGENTS.map((a) => a.id);
}

/** Resolve display meta for any key (catalog row or pure fallback). */
export function resolveAgentMeta(agentId: AgentId): AgentMeta {
  const hit = AGENT_MAP[agentId];
  if (hit) return hit;
  return {
    id: agentId,
    name: agentId,
    color: colorFor(agentId),
    letter: letterFor(agentId, agentId),
    installChannels: [],
  };
}

/** Safe display name; never throws on unknown catalog keys. */
export function agentDisplayName(agentId: AgentId): string {
  return resolveAgentMeta(agentId).name;
}
