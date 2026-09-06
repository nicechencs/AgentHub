import type { MouseEvent } from 'react';
import MarkdownPreview from '@uiw/react-markdown-preview';
import rehypeRaw from 'rehype-raw';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useTheme } from '@/components/shared/ThemeProvider';
import { useToast } from '@/components/ui/toast';
import { isHttpUrl, openExternalLink, openLocalPath } from '@/lib/open-external';
import { isAbsoluteFsPath, normalizeOpenPath } from '@/lib/path-open';
import { resolveTheme } from '@/lib/theme';
import { cn } from '@/lib/utils';

export type MarkdownViewVariant = 'chat' | 'document';

/** Override @uiw markdown.css 6px / square table chrome with design tokens. */
export const MARKDOWN_TOKEN_CHROME = [
  '[&_pre]:!overflow-x-auto [&_pre]:!rounded-card',
  '[&_pre_.copied]:!rounded-btn',
  '[&_code]:!rounded-btn [&_tt]:!rounded-btn [&_kbd]:!rounded-btn',
  // Clip the grid inside a bordered shell so cell edges are not bitten by radius.
  '[&_.md-table-shell]:my-2.5 [&_.md-table-shell]:w-max [&_.md-table-shell]:max-w-full',
  '[&_.md-table-shell]:overflow-x-auto [&_.md-table-shell]:rounded-card [&_.md-table-shell]:border [&_.md-table-shell]:border-border [&_.md-table-shell]:bg-panel',
  '[&_.md-table-shell_table]:!my-0 [&_.md-table-shell_table]:!w-full [&_.md-table-shell_table]:border-collapse [&_.md-table-shell_table]:border-hidden',
].join(' ');

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
  /** Resolve relative file links against this directory (chat working directory). */
  localBasePath?: string;
  /** Return true when the local path was handled (e.g. opened in the preview pane). */
  onOpenLocal?: (path: string) => boolean;
}

/** Minimal HAST element shape used by rehypeRewrite (avoids depending on `hast` types). */
type HastNode = {
  type?: string;
  tagName?: string;
  children?: HastNode[];
  properties?: Record<string, unknown>;
};

const BLOCKED_TAGS = new Set([
  'base',
  'button',
  'embed',
  'form',
  'iframe',
  'input',
  'link',
  'meta',
  'object',
  'script',
  'select',
  'style',
  'textarea',
  'video',
]);

/**
 * Markdown links/images are untrusted input. Allow ordinary web URLs and
 * same-document/relative links, but reject every other URI scheme (including
 * javascript:, data:, file:, and custom protocol handlers).
 */
export function isSafeMarkdownUrl(url: string): boolean {
  let candidate = url.trim();
  if (!candidate) return false;

  // Decode a couple of layers so encoded `javascript:` cannot bypass the
  // scheme check. Invalid percent escapes are treated as unsafe.
  for (let i = 0; i < 2; i += 1) {
    try {
      const decoded = decodeURIComponent(candidate);
      if (decoded === candidate) break;
      candidate = decoded;
    } catch {
      return false;
    }
  }
  candidate = candidate.replace(/[\u0000-\u001f\u007f]/g, '').trim();
  // Browsers normalize backslashes in special URLs, so `\\\\host` can act
  // like a protocol-relative URL. Reject them before the webview resolves the
  // relative href.
  if (!candidate || candidate.startsWith('//') || candidate.includes('\\')) return false;

  const scheme = candidate.match(/^([a-z][a-z\d+.-]*):/i)?.[1]?.toLowerCase();
  if (scheme) {
    return (scheme === 'http' || scheme === 'https') && /^https?:\/\//i.test(candidate);
  }
  return true;
}

function markdownHrefPath(url: string): string {
  return (url.trim().split('#')[0]?.split('?')[0] ?? '').trim();
}

/** Keep href only when a click can actually open or scroll something. */
export function isActionableMarkdownHref(url: string): boolean {
  if (!isSafeMarkdownUrl(url)) return false;
  const href = url.trim();
  if (href.startsWith('#')) return true;
  if (isHttpUrl(href) || /^www\./i.test(href)) return true;
  return looksLikeMarkdownLocalPath(href);
}

/** File/folder links in agent markdown — not site-style paths like `/docs/setup`. */
export function looksLikeMarkdownLocalPath(url: string): boolean {
  if (!isSafeMarkdownUrl(url)) return false;
  const path = markdownHrefPath(url);
  if (!path || path.startsWith('#') || isHttpUrl(path)) return false;
  if (path === '~' || path.startsWith('~/')) return true;
  if (path.startsWith('./') || path.startsWith('../')) return true;
  if (isAbsoluteFsPath(path)) {
    if (/\.[a-zA-Z0-9]{1,8}$/.test(path)) return true;
    const segs = path.split(/[\\/]/).filter(Boolean);
    return segs.length >= 3 || /^[A-Za-z]:/.test(path);
  }
  if (path.includes('/') || path.includes('\\')) return true;
  return /\.[a-zA-Z0-9]{1,8}$/.test(path);
}

export function joinLocalBasePath(basePath: string, rel: string): string {
  const base = basePath.trim().replace(/[\\/]+$/, '');
  let relNorm = rel.trim().replace(/\\/g, '/');
  while (relNorm.startsWith('./')) relNorm = relNorm.slice(2);
  const win = /^[A-Za-z]:/.test(base) || base.includes('\\');
  const sep = win ? '\\' : '/';
  return `${base}${sep}${relNorm.replace(/\//g, sep)}`;
}

export function resolveMarkdownLocalPath(href: string, basePath?: string): string | null {
  if (!looksLikeMarkdownLocalPath(href)) return null;
  const path = markdownHrefPath(href);
  if (!path) return null;
  if (path === '~' || path.startsWith('~/')) return path;
  if (isAbsoluteFsPath(path)) return normalizeOpenPath(path) ?? path;
  const base = basePath?.trim();
  if (!base) return null;
  const joined = joinLocalBasePath(base, path);
  return normalizeOpenPath(joined) ?? joined;
}

export function isMarkdownFilePath(path: string): boolean {
  return /\.(md|mdx|markdown)$/i.test(markdownHrefPath(path));
}

export function localParentDir(path: string): string {
  const raw = path.trim();
  if (!raw) return '';
  const win = /^[A-Za-z]:/.test(raw) || raw.includes('\\');
  const sep = win ? '\\' : '/';
  const normalized = win ? raw.replace(/\//g, '\\') : raw.replace(/\\/g, '/');
  const i = normalized.lastIndexOf(sep);
  if (i <= 0) return normalized;
  if (win && /^[A-Za-z]:\\$/.test(normalized.slice(0, i + 1))) {
    return normalized.slice(0, i + 1);
  }
  return normalized.slice(0, i);
}

/** Remove unsafe URL/HTML properties from one HAST node. */
export function sanitizeMarkdownNode(
  node: HastNode,
  _index?: number,
  parent?: HastNode,
) {
  const tagName = node.tagName?.toLowerCase();
  if (tagName && BLOCKED_TAGS.has(tagName)) {
    if (Array.isArray(parent?.children)) {
      parent.children = parent.children.filter((child) => child !== node);
    }
    return;
  }

  const properties = node.properties;
  if (!properties) return;
  for (const key of Object.keys(properties)) {
    const value = properties[key];
    if (/^on[a-z]/i.test(key)) {
      delete properties[key];
      continue;
    }
    if ((key === 'href' || key === 'src' || key === 'cite') && typeof value === 'string') {
      const ok = key === 'href' ? isActionableMarkdownHref(value) : isSafeMarkdownUrl(value);
      if (!ok) delete properties[key];
    }
  }
}

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

function classList(value: unknown): string[] {
  if (Array.isArray(value)) return value.map(String);
  if (typeof value === 'string') return value.split(/\s+/).filter(Boolean);
  return [];
}

/** Wrap `<table>` so radius lives on a bordered shell, not on clipped cell borders. */
export function wrapMarkdownTable(node: HastNode, index?: number, parent?: HastNode) {
  if (node.tagName !== 'table' || !parent?.children || typeof index !== 'number') return;
  if (classList(parent.properties?.className).includes('md-table-shell')) return;
  parent.children[index] = {
    type: 'element',
    tagName: 'div',
    properties: { className: ['md-table-shell'] },
    children: [node],
  };
}

function rewriteMarkdownNode(node: HastNode, index?: number, parent?: HastNode) {
  sanitizeMarkdownNode(node, index, parent);
  stripHeadingPermalink(node, index, parent);
  wrapMarkdownTable(node, index, parent);
}

/**
 * The preview package injects `rehype-raw` in its default entry point. Remove
 * every copy so untrusted HTML is never parsed into renderable elements.
 */
export const filterUnsafeMarkdownPlugins: NonNullable<
  React.ComponentProps<typeof MarkdownPreview>['pluginsFilter']
> = (type, plugins) => {
  if (type !== 'rehype') return plugins;
  return plugins.filter((entry) => {
    const plugin = Array.isArray(entry) ? entry[0] : entry;
    return plugin !== rehypeRaw;
  });
};

/**
 * Scroll a same-document markdown anchor without mutating the URL. HashRouter
 * owns `location.hash`, so letting the browser handle `#section` would replace
 * the application route. Missing targets are intentionally a no-op.
 */
export function scrollMarkdownAnchor(href: string): boolean {
  if (!href.startsWith('#') || typeof document === 'undefined') return false;
  let id: string;
  try {
    id = decodeURIComponent(href.slice(1));
  } catch {
    return false;
  }
  if (!id) return false;
  const target = document.getElementById(id);
  if (!target) return false;
  target.scrollIntoView({ behavior: 'smooth', block: 'start' });
  return true;
}

function clickElement(target: EventTarget | null): Element | null {
  if (!target) return null;
  if (typeof (target as Element).closest === 'function') return target as Element;
  return (target as Node).parentElement;
}

export type MarkdownClickOptions = {
  localBasePath?: string;
  onError?: (err: unknown) => void;
  onOpenLocal?: (path: string) => boolean;
};

/** Handle clicks before the webview/browser gets a chance to navigate. */
export function handleMarkdownClick(
  e: MouseEvent<HTMLElement>,
  options?: MarkdownClickOptions,
): void {
  const el = clickElement(e.target);
  if (!el) return;
  const anchor = el.closest('a');
  if (!anchor || !e.currentTarget.contains(anchor)) return;

  const href = anchor.getAttribute('href')?.trim() ?? '';
  // Intercept every markdown anchor so the webview never navigates in-place.
  e.preventDefault();
  e.stopPropagation();
  if (!href || !isSafeMarkdownUrl(href)) return;

  if (href.startsWith('#')) {
    scrollMarkdownAnchor(href);
    return;
  }

  const http = isHttpUrl(href) ? href : /^www\./i.test(href) ? `https://${href}` : null;
  if (http) {
    void openExternalLink(http).catch((err) => {
      console.error('[MarkdownView] open external failed', err);
      options?.onError?.(err);
    });
    return;
  }

  const local = resolveMarkdownLocalPath(href, options?.localBasePath);
  if (local) {
    if (options?.onOpenLocal?.(local)) return;
    void openLocalPath(local).catch((err) => {
      console.error('[MarkdownView] open local failed', err);
      options?.onError?.(err);
    });
    return;
  }

  options?.onError?.(new Error(href));
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
  localBasePath,
  onOpenLocal,
}: MarkdownViewProps) {
  const { theme } = useTheme();
  const { t } = useI18n();
  const { toast } = useToast();
  const colorMode = resolveTheme(theme);
  const text = content ?? '';
  if (!text.trim()) return null;

  return (
    <div
      onClickCapture={(event) => {
        handleMarkdownClick(event, {
          localBasePath,
          onOpenLocal,
          onError: () => toast({ title: t('common.openLinkFailed'), variant: 'danger' }),
        });
      }}
    >
      <MarkdownPreview
        source={text}
        // v5 inverts this prop before passing it to react-markdown. Combined
        // with the plugin filter, `false` means raw nodes are skipped.
        skipHtml={false}
        pluginsFilter={filterUnsafeMarkdownPlugins}
        urlTransform={(url) => (isSafeMarkdownUrl(url) ? url.trim() : '')}
        allowElement={(element) => !BLOCKED_TAGS.has(element.tagName.toLowerCase())}
        wrapperElement={{ 'data-color-mode': colorMode }}
        // Must stay disabled while HashRouter owns the URL fragment.
        rehypeRewrite={rewriteMarkdownNode}
        className={cn(
          // Reset library default canvas so it inherits chat/dialog backgrounds.
          '!bg-transparent',
          MARKDOWN_TOKEN_CHROME,
          variant === 'chat' &&
            'text-body [&_pre]:text-meta [&_a]:!text-accent [&_a]:!no-underline hover:[&_a]:!underline',
          // document：三档字号；! 覆盖 @uiw 默认 2em h1 / 底部分割线，避免压过预览 chrome
          variant === 'document' &&
            [
              'wmde-markdown-document min-w-0 max-w-full break-words',
              'text-body leading-[1.45]',
              '[&>:first-child]:!mt-0',
              // h1 走 title 档，去掉 GitHub 风底边线
              '[&_h1]:!mt-0 [&_h1]:!mb-2 [&_h1]:!border-0 [&_h1]:!pb-0',
              '[&_h1]:!text-title [&_h1]:!font-semibold [&_h1]:!leading-[1.35] [&_h1]:!text-primary',
              '[&_h2]:!mt-4 [&_h2]:!mb-1.5 [&_h2]:!border-0 [&_h2]:!pb-0',
              '[&_h2]:!text-body [&_h2]:!font-semibold [&_h2]:!leading-[1.35] [&_h2]:!text-primary',
              '[&_h3]:!mt-3 [&_h3]:!mb-1 [&_h3]:!text-body [&_h3]:!font-semibold [&_h3]:!text-primary',
              '[&_h4]:!mt-2.5 [&_h4]:!mb-1 [&_h4]:!text-body [&_h4]:!font-medium [&_h4]:!text-primary',
              '[&_p]:!my-1.5 [&_p]:!text-body [&_p]:!leading-[1.45] [&_p]:!text-primary',
              '[&_ul]:!my-1.5 [&_ol]:!my-1.5 [&_li]:!my-0.5 [&_li]:!text-body',
              '[&_pre]:!my-2.5 [&_pre]:!max-w-full [&_pre]:!text-meta',
              '[&_code]:!text-meta [&_code]:!font-mono',
              '[&_blockquote]:!my-2 [&_blockquote]:!border-l-2 [&_blockquote]:!border-border [&_blockquote]:!pl-3 [&_blockquote]:!text-secondary',
              '[&_hr]:!my-3 [&_hr]:!border-border',
              '[&_.md-table-shell]:!my-2.5',
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
          fontSize: 'var(--font-body-size, 13px)',
          padding: 0,
        }}
      />
    </div>
  );
}
