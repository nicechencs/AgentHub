import { useState } from 'react';
import { resolveAgentMeta } from '@/config/agents';
import type { AgentKey } from '@/lib/types';
import { Hint } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { AGENT_COLORS, AGENT_CSS_VAR_TO_HEX_LIGHT, type TokenAgentId } from '@/styles/tokens';

/** 解析 CSS 变量或 hex，返回用于对比度判断的亮度 (0–1) */
function relativeLuminance(color: string): number {
  // hex 真源：src/styles/tokens.ts（禁止再维护一份品牌色表）
  let hex = AGENT_CSS_VAR_TO_HEX_LIGHT[color] ?? color;
  if (hex.startsWith('var(')) hex = '#888888';
  const raw = hex.replace('#', '');
  const full =
    raw.length === 3
      ? raw
          .split('')
          .map((c) => c + c)
          .join('')
      : raw;
  if (full.length !== 6) return 0.5;
  const r = parseInt(full.slice(0, 2), 16) / 255;
  const g = parseInt(full.slice(2, 4), 16) / 255;
  const b = parseInt(full.slice(4, 6), 16) / 255;
  const toLin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * toLin(r) + 0.7152 * toLin(g) + 0.0722 * toLin(b);
}

function brandHex(agentId: AgentKey): string | undefined {
  if (agentId in AGENT_COLORS) {
    return AGENT_COLORS[agentId as TokenAgentId].light;
  }
  return undefined;
}

/** 展示 agent 本地 logo；未知 agent 或 logo 加载失败时回退为首字母方标。 */
export function AgentLogo({ agentId, size = 'md' }: { agentId: AgentKey; size?: 'sm' | 'md' | 'lg' }) {
  const meta = resolveAgentMeta(agentId);
  const sizeCls = {
    sm: 'h-6 w-6 text-meta',
    md: 'h-8 w-8 text-xs',
    lg: 'h-10 w-10 text-sm',
  }[size];
  // 未知 agent：resolveAgentMeta 已用 muted + 首字母 fallback
  const color = meta.color;
  const letter = meta.letter;
  const name = meta.name;
  const svgLogoSrc = meta.logoSvgSrc;
  const pngLogoSrc = meta.logoSrc;
  const logoBackground = meta.logoBackground ?? '#ffffff';

  type LogoLoadState = {
    agentId: AgentKey;
    svgSrc?: string;
    pngSrc?: string;
    svgFailed: boolean;
    pngFailed: boolean;
  };

  // Keep failures scoped to the exact agent/source pair. A different
  // agent—or a refreshed asset URL—must get a fresh chance to render its logo.
  const [logoState, setLogoState] = useState<LogoLoadState>({
    agentId,
    svgSrc: svgLogoSrc,
    pngSrc: pngLogoSrc,
    svgFailed: false,
    pngFailed: false,
  });
  const sameAssetSet =
    logoState.agentId === agentId &&
    logoState.svgSrc === svgLogoSrc &&
    logoState.pngSrc === pngLogoSrc;
  const svgFailed = sameAssetSet && logoState.svgFailed;
  const pngFailed = sameAssetSet && logoState.pngFailed;
  const logoSrc = svgLogoSrc && !svgFailed ? svgLogoSrc : !pngFailed ? pngLogoSrc : undefined;
  const logoKind = logoSrc === svgLogoSrc ? 'svg' : logoSrc === pngLogoSrc ? 'png' : undefined;
  const showLogo = Boolean(logoSrc);
  const lightBg = relativeLuminance(brandHex(agentId) ?? color) > 0.55;
  return (
    <Hint label={name}>
      <span
        className={cn(
          'inline-flex shrink-0 items-center justify-center overflow-hidden rounded-mark font-bold',
          showLogo
            ? 'border border-border p-0.5'
            : lightBg
              ? 'text-primary'
              : 'text-white',
          sizeCls,
        )}
        style={{ backgroundColor: showLogo ? logoBackground : color }}
        aria-label={name}
      >
        {logoSrc && showLogo ? (
          <img
            src={logoSrc}
            alt=""
            aria-hidden="true"
            className="h-full w-full rounded-mark object-contain"
            onError={() => {
              setLogoState((previous) => {
                // Ignore a stale error from an image that belonged to an
                // earlier agent/source after props changed.
                const current =
                  previous.agentId === agentId &&
                  previous.svgSrc === svgLogoSrc &&
                  previous.pngSrc === pngLogoSrc;
                if (!current) {
                  // Props can change before React has committed a state
                  // update. Seed the new source pair from this error event so
                  // its fallback chain still works on the next render.
                  return {
                    agentId,
                    svgSrc: svgLogoSrc,
                    pngSrc: pngLogoSrc,
                    svgFailed: logoKind === 'svg',
                    pngFailed: logoKind === 'png',
                  };
                }
                if (logoKind === 'svg') return { ...previous, svgFailed: true };
                if (logoKind === 'png') return { ...previous, pngFailed: true };
                return previous;
              });
            }}
          />
        ) : (
          letter
        )}
      </span>
    </Hint>
  );
}
