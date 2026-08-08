import type { MouseEvent } from 'react';
import MarkdownPreview from '@uiw/react-markdown-preview';
import { useTheme } from '@/components/shared/ThemeProvider';
import { isHttpUrl, openExternalLink } from '@/lib/open-external';
import { resolveTheme } from '@/lib/theme';
import { cn } from '@/lib/utils';

export type MarkdownViewVariant = 'chat' | 'document';

export interface MarkdownViewProps {
  /** Markdown source. Empty / whitespace-only → render nothing. */
  content: string;
  className?: string;
  /**
   * Density:
   * - `chat` — message body (compact)
   * - `document` — skill / file preview (IDE-dense; not GitHub README scale)
   */
  variant?: MarkdownViewVariant;
}

/** Minimal HAST element shape used by rehypeRewrite (avoids depending on `hast` types). */
type HastNode = { type?: string; tagName?: string; children?: HastNode[] };

/**
 * Strip GitHub-style heading permalink anchors.
 *
 * `@uiw/react-markdown-preview` injects `a[href="#slug"]` on h1–h6. This app
 * uses `HashRouter`, so those hashes replace the real route (`#/chat` →
 * `#heading`) and React Router renders no match → white screen.
 *
 * Official library recipe: remove the first child (`a`) under each heading.
 */
function stripHeadingPermalink(
  node: HastNode,
  _index?: number,
  parent?: HastNode,
) {
  if (
    node.tagName === 'a' &&
    parent?.tagName &&
    /^h[1-6]$/.test(parent.tagName) &&
    Array.isArray(parent.children)
  ) {
    parent.children = parent.children.slice(1);
  }
}

/**
 * Shared markdown preview powered by `@uiw/react-markdown-preview`
 * (same family as the project's CodeMirror). GFM + code highlight + dark/light
 * come from the library; this wrapper only binds theme tokens and density.
 */
export function MarkdownView({
  content,
  className,
  variant = 'chat',
}: MarkdownViewProps) {
  const { theme } = useTheme();
  const colorMode = resolveTheme(theme);
  const text = content ?? '';
  if (!text.trim()) return null;

  const onMarkdownClick = (e: MouseEvent<HTMLDivElement>) => {
    const el = e.target;
    if (!(el instanceof Element)) return;
    const a = el.closest('a');
    if (!a || !e.currentTarget.contains(a)) return;
    const href = a.getAttribute('href')?.trim() ?? '';
    if (!href || href.startsWith('#') || href.startsWith('/')) return;
    if (!isHttpUrl(href)) return;
    // Tauri webview: open system browser instead of navigating in-app.
    e.preventDefault();
    e.stopPropagation();
    void openExternalLink(href).catch((err) => {
      console.error('[MarkdownView] open external failed', err);
    });
  };

  return (
    <div onClick={onMarkdownClick}>
      <MarkdownPreview
        source={text}
        wrapperElement={{ 'data-color-mode': colorMode }}
        // Must stay disabled while HashRouter owns the URL fragment.
        rehypeRewrite={stripHeadingPermalink}
        className={cn(
          // Reset library default canvas so it inherits chat/dialog backgrounds.
          '!bg-transparent',
          variant === 'chat' && 'text-sm [&_pre]:text-2xs',
          // document：侧栏密度；! 覆盖 @uiw 默认 2em h1 / 底部分割线，避免压过预览 chrome
          variant === 'document' &&
            [
              'wmde-markdown-document min-w-0 max-w-full break-words',
              'text-sm leading-[1.45]',
              '[&>:first-child]:!mt-0',
              // h1 16px（与页内区块标题同级），去掉 GitHub 风底边线
              '[&_h1]:!mt-0 [&_h1]:!mb-2 [&_h1]:!border-0 [&_h1]:!pb-0',
              '[&_h1]:!text-base [&_h1]:!font-semibold [&_h1]:!leading-[1.35] [&_h1]:!text-primary',
              '[&_h2]:!mt-4 [&_h2]:!mb-1.5 [&_h2]:!border-0 [&_h2]:!pb-0',
              '[&_h2]:!text-sm [&_h2]:!font-semibold [&_h2]:!leading-[1.35] [&_h2]:!text-primary',
              '[&_h3]:!mt-3 [&_h3]:!mb-1 [&_h3]:!text-sm [&_h3]:!font-semibold [&_h3]:!text-primary',
              '[&_h4]:!mt-2.5 [&_h4]:!mb-1 [&_h4]:!text-sm [&_h4]:!font-medium [&_h4]:!text-primary',
              '[&_p]:!my-1.5 [&_p]:!text-sm [&_p]:!leading-[1.5] [&_p]:!text-primary',
              '[&_ul]:!my-1.5 [&_ol]:!my-1.5 [&_li]:!my-0.5 [&_li]:!text-sm',
              '[&_pre]:!my-2.5 [&_pre]:!max-w-full [&_pre]:!overflow-x-auto [&_pre]:!rounded-btn [&_pre]:!text-xs',
              '[&_code]:!text-xs [&_code]:!font-mono',
              '[&_blockquote]:!my-2 [&_blockquote]:!border-l-2 [&_blockquote]:!border-border [&_blockquote]:!pl-3 [&_blockquote]:!text-secondary',
              '[&_hr]:!my-3 [&_hr]:!border-border',
              '[&_table]:!my-2.5 [&_table]:!w-max [&_table]:!max-w-none [&_table]:!border-collapse',
              '[&_th]:!px-2 [&_th]:!py-1.5 [&_th]:!text-left [&_td]:!px-2 [&_td]:!py-1.5',
              '[&_img]:!max-w-full [&_img]:!h-auto',
              // 链接用 accent，避免库默认亮蓝与 indigo 体系打架
              '[&_a]:!text-accent [&_a]:!no-underline hover:[&_a]:!underline',
            ].join(' '),
          className,
        )}
        style={{
          // Keep text on design tokens; library handles internal structure.
          backgroundColor: 'transparent',
          color: 'var(--text-primary)',
          fontSize: 13,
          padding: 0,
        }}
      />
    </div>
  );
}
