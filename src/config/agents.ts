/**
 * Agent UI meta façade.
 *
 * Product agent set (id / name / channels / capabilities) comes from the
 * runtime Agent Catalog (`applyAgentCatalog`). This module only keeps pure
 * display tokens (letter / brand color) for known agents; unknown keys get
 * neutral fallback styling.
 */
import type { AgentCapabilities } from '@/lib/capability';
import type {
  AgentCatalogEntryDto,
  LiveOccupancyDto,
} from '@/lib/backend/contracts/agent-catalog-types';
import {
  catalogOccupancy,
  mapCatalogCapabilities,
} from '@/lib/backend/contracts/agent-catalog-types';
import type { AgentId, RuntimeId } from '@/lib/types';
import { agentCssVar, type TokenAgentId } from '@/styles/tokens';
import claudeLogo from '@/assets/agent-logos/claude.png';
import codexLogo from '@/assets/agent-logos/codex.png';
import cursorLogo from '@/assets/agent-logos/cursor.png';
import deepseekLogo from '@/assets/agent-logos/deepseek.png';
import grokLogo from '@/assets/agent-logos/grok.png';
import kimiLogo from '@/assets/agent-logos/kimi.png';
import piLogo from '@/assets/agent-logos/pi.png';
import workbuddyLogo from '@/assets/agent-logos/workbuddy.png';
import zcodeLogo from '@/assets/agent-logos/zcode.png';

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
  /** 首字母回退显示 */
  letter: string;
  /** 对应 agent 的本地 logo 图片；未知 agent 没有图片 */
  logoSrc?: string;
  /**
   * Install channels from backend catalog.
   * Empty until `applyAgentCatalog` / hydrate runs.
   */
  installChannels: InstallChannelMeta[];
  /** Catalog-declared capabilities (optional; doctor status may override). */
  capabilities?: AgentCapabilities;
  /** How a live write occupies this agent's config. Missing → exclusive. */
  occupancy: LiveOccupancyDto;
}

/** Pure display decoration for known agents — not the product set. */
export const AGENT_DISPLAY: Readonly<
  Record<
    string,
    { readonly letter: string; readonly colorKey: TokenAgentId; readonly logoSrc?: string }
  >
> = Object.freeze({
  claude: { letter: 'C', colorKey: 'claude', logoSrc: claudeLogo },
  codex: { letter: 'X', colorKey: 'codex', logoSrc: codexLogo },
  kimi: { letter: 'K', colorKey: 'kimi', logoSrc: kimiLogo },
  grok: { letter: 'G', colorKey: 'grok', logoSrc: grokLogo },
  pi: { letter: 'P', colorKey: 'pi', logoSrc: piLogo },
  workbuddy: { letter: 'W', colorKey: 'workbuddy', logoSrc: workbuddyLogo },
  cursor: { letter: 'R', colorKey: 'cursor', logoSrc: cursorLogo },
  dsh: { letter: 'D', colorKey: 'dsh', logoSrc: deepseekLogo },
  zcode: { letter: 'Z', colorKey: 'zcode', logoSrc: zcodeLogo },
});

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
    logoSrc: AGENT_DISPLAY[entry.key]?.logoSrc,
    installChannels: entry.installChannels.map((ch) => ({
      id: ch.id,
      label: ch.label,
      command: ch.command,
      requires: ch.requires,
    })),
    capabilities: mapCatalogCapabilities(entry.capabilities),
    occupancy: catalogOccupancy(entry.occupancy),
  };
}

/**
 * Runtime agent list — filled only by {@link applyAgentCatalog}.
 * Not a static closed set; starts empty until catalog load/seed.
 * The snapshot is frozen; replace it via {@link applyAgentCatalog}.
 */
export let AGENTS: readonly AgentMeta[] = Object.freeze([]);

/** Lookup map; replaced wholesale with {@link applyAgentCatalog}. */
export let AGENT_MAP: Readonly<Record<string, AgentMeta>> = Object.freeze(
  Object.create(null) as Record<string, AgentMeta>,
);

/** Convenience id list; mirrors AGENTS order. */
export let AGENT_IDS: readonly AgentId[] = Object.freeze([]);

/**
 * Replace product agent set from backend catalog entries.
 * Call sites that need the full list must run after this (boot / mock seed).
 */
export function applyAgentCatalog(entries: AgentCatalogEntryDto[]): void {
  const next: AgentMeta[] = [];
  const map: Record<string, AgentMeta> = Object.create(null);
  for (const entry of entries) {
    const meta = agentMetaFromCatalogEntry(entry);
    Object.freeze(meta.installChannels);
    Object.freeze(meta);
    next.push(meta);
    map[meta.id] = meta;
  }
  AGENTS = Object.freeze(next);
  AGENT_MAP = Object.freeze(map);
  AGENT_IDS = Object.freeze(next.map((a) => a.id));
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
    logoSrc: AGENT_DISPLAY[agentId]?.logoSrc,
    installChannels: [],
    occupancy: 'exclusive',
  };
}

/** Safe display name; never throws on unknown catalog keys. */
export function agentDisplayName(agentId: AgentId): string {
  return resolveAgentMeta(agentId).name;
}
