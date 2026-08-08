import * as React from 'react';
import { cn } from '@/lib/utils';
import { AGENT_COLOR_VARS } from '@/styles/tokens';

const EXIT_MS = 420;

export type BootSplashProps = {
  /** 进入退场阶段：淡出 + 微缩放 */
  exiting?: boolean;
  onExited?: () => void;
  className?: string;
};

/**
 * 启动遮罩：与 index.html `#boot-fallback` 视觉对齐。
 * React 挂载后接管，就绪后 `exiting` 淡出，避免白屏/硬切。
 */
export function BootSplash({ exiting = false, onExited, className }: BootSplashProps) {
  React.useEffect(() => {
    if (!exiting) return;
    const t = window.setTimeout(() => onExited?.(), EXIT_MS);
    return () => window.clearTimeout(t);
  }, [exiting, onExited]);

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
      aria-label="AgentHub 正在启动"
    >
      {/* 柔光背景 */}
      <div className="boot-splash__glow pointer-events-none absolute inset-0" aria-hidden />

      <div className="boot-splash__content relative flex flex-col items-center gap-5">
        {/* Logo mark */}
        <div className="boot-splash__mark relative flex h-14 w-14 items-center justify-center">
          <span className="boot-splash__mark-ring absolute inset-0 rounded-2xl" aria-hidden />
          <span className="relative flex h-12 w-12 items-center justify-center rounded-xl bg-subtle text-secondary shadow-sm">
            <BootMark className="h-6 w-6" />
          </span>
        </div>

        <div className="flex flex-col items-center gap-1.5">
          <div className="boot-splash__title text-lg font-semibold tracking-tight text-primary">
            AgentHub
          </div>
          <div className="boot-splash__subtitle text-xs text-muted">
            {exiting ? '就绪' : '正在启动…'}
          </div>
        </div>

        {/* Agent 品牌色点：依次点亮 */}
        <div className="boot-splash__dots flex items-center gap-1.5" aria-hidden>
          {AGENT_COLOR_VARS.map((color, i) => (
            <span
              key={color}
              className="boot-splash__dot h-1.5 w-1.5 rounded-full"
              style={
                {
                  backgroundColor: color,
                  animationDelay: `${180 + i * 70}ms`,
                } as React.CSSProperties
              }
            />
          ))}
        </div>

        {/* 进度条 */}
        <div
          className="boot-splash__bar relative h-0.5 w-28 overflow-hidden rounded-full bg-border"
          aria-hidden
        >
          <span
            className={cn(
              'boot-splash__bar-fill absolute inset-y-0 left-0 rounded-full bg-accent',
              exiting && 'boot-splash__bar-fill--done',
            )}
          />
        </div>
      </div>
    </div>
  );
}

/** 与侧栏 Hexagon 一致的简化 mark（SVG，无 lucide 依赖，便于与 HTML 兜底对齐） */
function BootMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
    >
      <path d="M12 2.5 20 7v10l-8 4.5L4 17V7l8-4.5Z" />
      <path d="M12 8v8" opacity="0.35" />
      <path d="M8.5 10.5 12 12.5l3.5-2" opacity="0.55" />
    </svg>
  );
}
