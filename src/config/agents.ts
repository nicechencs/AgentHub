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
import claudeLogoSvg from '@/assets/agent-logos/claude.svg';
import codexLogoSvg from '@/assets/agent-logos/codex.svg';
import cursorLogo from '@/assets/agent-logos/cursor.png';
import cursorLogoSvg from '@/assets/agent-logos/cursor.svg';
import deepseekLogo from '@/assets/agent-logos/deepseek.png';
import deepseekLogoSvg from '@/assets/agent-logos/deepseek.svg';
import grokLogo from '@/assets/agent-logos/grok.png';
import grokLogoSvg from '@/assets/agent-logos/grok.svg';
import kimiLogoSvg from '@/assets/agent-logos/kimi.svg';
import piLogo from '@/assets/agent-logos/pi.png';
import piLogoSvg from '@/assets/agent-logos/pi.svg';
import workbuddyLogoSvg from '@/assets/agent-logos/workbuddy.svg';
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
  /** 对应 agent 的本地 PNG logo 兜底；未知 agent 没有图片 */
  logoSrc?: string;
  /** 对应 agent 的本地 SVG logo 首选；未知 agent 没有图片 */
  logoSvgSrc?: string;
  /** logo 图片容器的本地对比背景色；首字母回退不使用该值 */
  logoBackground?: string;
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
    {
      readonly letter: string;
      readonly colorKey: TokenAgentId;
      readonly logoSrc?: string;
      readonly logoSvgSrc?: string;
      readonly logoBackground?: string;
    }
  >
> = Object.freeze({
  claude: {
    letter: 'C',
    colorKey: 'claude',
    logoSrc: claudeLogo,
    logoSvgSrc: claudeLogoSvg,
    logoBackground: '#ffffff',
  },
  codex: {
    letter: 'X',
    colorKey: 'codex',
    logoSvgSrc: codexLogoSvg,
    logoBackground: '#ffffff',
  },
  kimi: {
    letter: 'K',
    colorKey: 'kimi',
    logoSvgSrc: kimiLogoSvg,
    logoBackground: '#ffffff',
  },
  grok: {
    letter: 'G',
    colorKey: 'grok',
    logoSrc: grokLogo,
    logoSvgSrc: grokLogoSvg,
    logoBackground: '#000000',
  },
  pi: {
    letter: 'P',
    colorKey: 'pi',
    logoSrc: piLogo,
    logoSvgSrc: piLogoSvg,
    logoBackground: '#ffffff',
  },
  workbuddy: {
    letter: 'W',
    colorKey: 'workbuddy',
    logoSvgSrc: workbuddyLogoSvg,
    logoBackground: '#ffffff',
  },
  cursor: {
    letter: 'R',
    colorKey: 'cursor',
    logoSrc: cursorLogo,
    logoSvgSrc: cursorLogoSvg,
    logoBackground: '#111111',
  },
  dsh: {
    letter: 'D',
    colorKey: 'dsh',
    logoSrc: deepseekLogo,
    logoSvgSrc: deepseekLogoSvg,
    logoBackground: '#ffffff',
  },
  zcode: {
    letter: 'Z',
    colorKey: 'zcode',
    logoSrc: zcodeLogo,
    logoBackground: '#ffffff',
  },
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
    logoSvgSrc: AGENT_DISPLAY[entry.key]?.logoSvgSrc,
    logoBackground: AGENT_DISPLAY[entry.key]?.logoBackground,
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
    logoSvgSrc: AGENT_DISPLAY[agentId]?.logoSvgSrc,
    logoBackground: AGENT_DISPLAY[agentId]?.logoBackground,
    installChannels: [],
    occupancy: 'exclusive',
  };
}

/** Safe display name; never throws on unknown catalog keys. */
export function agentDisplayName(agentId: AgentId): string {
  return resolveAgentMeta(agentId).name;
}
