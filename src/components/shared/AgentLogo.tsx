import { useState } from 'react';
import { resolveAgentMeta } from '@/config/agents';
import type { AgentId } from '@/lib/types';
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

function brandHex(agentId: AgentId): string | undefined {
  if (agentId in AGENT_COLORS) {
    return AGENT_COLORS[agentId as TokenAgentId].light;
  }
  return undefined;
}

/** 展示 agent 本地 logo；未知 agent 或 logo 加载失败时回退为首字母圆标。 */
export function AgentLogo({ agentId, size = 'md' }: { agentId: AgentId; size?: 'sm' | 'md' | 'lg' }) {
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
  const logoSrc = meta.logoSrc;
  // Keep the error scoped to the source that failed. A different agent/source
  // must get a fresh chance to render its own logo after a prop change.
  const [logoState, setLogoState] = useState<{ src?: string; failed: boolean }>({
    src: logoSrc,
    failed: false,
  });
  const showLogo = Boolean(logoSrc && !(logoState.src === logoSrc && logoState.failed));
  const lightBg = relativeLuminance(brandHex(agentId) ?? color) > 0.55;
  return (
    <Hint label={name}>
      <span
        className={cn(
          'inline-flex shrink-0 items-center justify-center rounded-full font-bold',
          showLogo
            ? 'border border-border bg-white p-0.5 dark:bg-zinc-100'
            : lightBg
              ? 'text-primary'
              : 'text-white',
          sizeCls,
        )}
        style={showLogo ? undefined : { backgroundColor: color }}
        aria-label={name}
      >
        {logoSrc && showLogo ? (
          <img
            src={logoSrc}
            alt=""
            aria-hidden="true"
            className="h-full w-full rounded-full object-contain"
            onError={() => setLogoState({ src: logoSrc, failed: true })}
          />
        ) : (
          letter
        )}
      </span>
    </Hint>
  );
}
