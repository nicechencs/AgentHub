import { forwardRef, type HTMLAttributes, type ReactNode } from 'react';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { cn } from '@/lib/utils';

export type PageSectionProps = HTMLAttributes<HTMLElement> & {
  title?: string;
  description?: string;
  /**
   * 顶部加分割线的主内容大段（如 Dashboard「用量明细」）。
   * 默认 false：仅 `mt-6` 段距。
   */
  ruled?: boolean;
  /**
   * 页内第一段（紧接 PageHeader，不再加顶距）。
   * 默认 false。
   */
  first?: boolean;
  children?: ReactNode;
};

/**
 * 页面主内容段：统一段距 / 可选分割线 / 可选段标题。
 * 全高特例页（Chat / Skills）可不使用，自管布局。
 */
export const PageSection = forwardRef<HTMLElement, PageSectionProps>(
  (
    {
      title,
      description,
      ruled = false,
      first = false,
      className,
      children,
      ...props
    },
    ref,
  ) => {
    return (
      <section
        ref={ref}
        className={cn(
          !first && (ruled ? pageRhythm.sectionRuled : pageRhythm.section),
          pageRhythm.scrollMt,
          className,
        )}
        {...props}
      >
        {(title || description) && (
          <div className={pageRhythm.sectionHead}>
            {title ? (
              <h2 className="text-base font-semibold tracking-tight text-primary">{title}</h2>
            ) : null}
            {description ? (
              <p className={cn(title ? 'mt-0.5' : undefined, 'text-xs text-secondary')}>
                {description}
              </p>
            ) : null}
          </div>
        )}
        {children}
      </section>
    );
  },
);
PageSection.displayName = 'PageSection';
