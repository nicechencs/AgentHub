import * as React from 'react';
import { AgentDot } from '@/components/shared/AgentDot';
import { AppLogo } from '@/components/shared/AppLogo';
import { createTranslator, loadStoredLanguage } from '@/lib/i18n';
import { cn } from '@/lib/utils';
import { AGENT_COLOR_VARS } from '@/styles/tokens';

const EXIT_MS = 420;
/** 超过此时长仍未就绪时，提示用户仍在加载（避免“假死”感） */
const SLOW_HINT_MS = 1800;

export type BootSplashProps = {
  /** 进入退场阶段：淡出 + 微缩放 */
  exiting?: boolean;
  /** 0–1 真实/估算进度；缺省时走不确定滑动条 */
  progress?: number;
  onExited?: () => void;
  className?: string;
};

/**
 * 启动遮罩：与 index.html `#boot-fallback` 视觉对齐。
 * React 挂载后接管，就绪后 `exiting` 淡出，避免白屏/硬切。
 */
export function BootSplash({
  exiting = false,
  progress,
  onExited,
  className,
}: BootSplashProps) {
  const [slow, setSlow] = React.useState(false);
  const determinate =
    typeof progress === 'number' && Number.isFinite(progress) && progress >= 0;
  const pct = determinate ? Math.max(0, Math.min(1, progress)) : 0;

  React.useEffect(() => {
    if (!exiting) return;
    const t = window.setTimeout(() => onExited?.(), EXIT_MS);
    return () => window.clearTimeout(t);
  }, [exiting, onExited]);

  React.useEffect(() => {
    if (exiting) {
      setSlow(false);
      return;
    }
    const t = window.setTimeout(() => setSlow(true), SLOW_HINT_MS);
    return () => window.clearTimeout(t);
  }, [exiting]);

  const t = createTranslator(loadStoredLanguage());
  const subtitle = exiting
    ? t('chrome.boot.ready')
    : slow
      ? t('chrome.boot.slow')
      : t('chrome.boot.starting');

  return (
    <div
      className={cn(
        // handoff：React 接管 HTML 兜底时保持终态，不重播入场动画
        'boot-splash boot-splash--handoff fixed inset-0 z-[9999] flex flex-col items-center justify-center',
        'bg-canvas text-primary',
        exiting && 'boot-splash--exit',
        className,
      )}
      role="status"
      aria-live="polite"
      aria-busy={!exiting}
      aria-label={t('chrome.boot.aria')}
      aria-valuemin={determinate ? 0 : undefined}
      aria-valuemax={determinate ? 100 : undefined}
      aria-valuenow={determinate ? Math.round(pct * 100) : undefined}
    >
      {/* 柔光背景 */}
      <div className="boot-splash__glow pointer-events-none absolute inset-0" aria-hidden />

      <div className="boot-splash__content relative flex flex-col items-center gap-5">
        {/* Product logo（与安装包 / 桌面图标一致） */}
        <div className="boot-splash__mark relative flex h-14 w-14 items-center justify-center">
          <span className="boot-splash__mark-ring absolute inset-0 rounded-2xl" aria-hidden />
          <AppLogo
            size={48}
            alt="AgentHub"
            className="relative h-12 w-12 rounded-[22%] shadow-sm"
          />
        </div>

        <div className="flex flex-col items-center gap-1.5">
          <div className="boot-splash__title text-title font-semibold tracking-tight text-primary">
            AgentHub
          </div>
          <div className="boot-splash__subtitle text-xs text-muted">{subtitle}</div>
        </div>

        {/* Agent 品牌色点：handoff 下保留轻呼吸，避免“全静止” */}
        <div className="boot-splash__dots flex items-center gap-1.5" aria-hidden>
          {AGENT_COLOR_VARS.map((color, i) => (
            <AgentDot
              key={color}
              color={color}
              size="sm"
              title={null}
              className="boot-splash__dot boot-splash__dot--pulse"
              // Stagger via CSS var on the shared size class; delay lives on the node.
              style={{ animationDelay: `${i * 120}ms` } as React.CSSProperties}
            />
          ))}
        </div>

        {/* 进度条：优先确定进度；否则 indeterminate 滑动 */}
        <div
          className="boot-splash__bar relative h-0.5 w-28 overflow-hidden rounded-full bg-border"
          aria-hidden
        >
          {determinate ? (
            <span
              className={cn(
                'boot-splash__bar-fill boot-splash__bar-fill--determinate absolute inset-y-0 left-0 rounded-full bg-accent',
                exiting && 'boot-splash__bar-fill--done',
              )}
              style={{ width: `${Math.max(exiting ? 100 : pct * 100, 6)}%` }}
            />
          ) : (
            <span
              className={cn(
                'boot-splash__bar-fill boot-splash__bar-fill--indeterminate absolute inset-y-0 rounded-full bg-accent',
                exiting && 'boot-splash__bar-fill--done',
              )}
            />
          )}
        </div>
      </div>
    </div>
  );
}
