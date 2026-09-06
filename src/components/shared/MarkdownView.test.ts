import { describe, expect, it, vi } from 'vitest';
import rehypeRaw from 'rehype-raw';

const { openExternalLinkMock, openLocalPathMock } = vi.hoisted(() => ({
  openExternalLinkMock: vi.fn(),
  openLocalPathMock: vi.fn(),
}));

vi.mock('@/lib/open-external', () => ({
  isHttpUrl: (url: string) => /^https?:\/\//i.test(url.trim()),
  openExternalLink: openExternalLinkMock,
  openLocalPath: openLocalPathMock,
}));

import {
  MARKDOWN_TOKEN_CHROME,
  filterUnsafeMarkdownPlugins,
  handleMarkdownClick,
  isActionableMarkdownHref,
  isMarkdownFilePath,
  isSafeMarkdownUrl,
  localParentDir,
  looksLikeMarkdownLocalPath,
  resolveMarkdownLocalPath,
  sanitizeMarkdownNode,
  scrollMarkdownAnchor,
  wrapMarkdownTable,
} from './MarkdownView';

describe('MarkdownView token chrome', () => {
  it('overrides library 6px / square boxes with btn and card radii', () => {
    expect(MARKDOWN_TOKEN_CHROME).toContain('[&_pre]:!rounded-card');
    expect(MARKDOWN_TOKEN_CHROME).toContain('[&_code]:!rounded-btn');
    expect(MARKDOWN_TOKEN_CHROME).toContain('[&_kbd]:!rounded-btn');
    expect(MARKDOWN_TOKEN_CHROME).toContain('[&_.md-table-shell]:rounded-card');
    expect(MARKDOWN_TOKEN_CHROME).toContain('[&_.md-table-shell_table]:border-hidden');
    expect(MARKDOWN_TOKEN_CHROME).not.toContain('[&_table]:!overflow-hidden');
  });

  it('wraps tables once in a radius shell', () => {
    const table = { tagName: 'table', children: [] };
    const parent = { tagName: 'div', children: [table] };
    wrapMarkdownTable(table, 0, parent);
    // Structural mirror of MarkdownView's non-exported HastNode so the
    // shell can be passed back into wrapMarkdownTable below.
    type TestHastNode = {
      type?: string;
      tagName?: string;
      children?: TestHastNode[];
      properties?: Record<string, unknown>;
    };
    const shell = parent.children[0] as TestHastNode;
    expect(shell.tagName).toBe('div');
    expect(shell.properties?.className).toEqual(['md-table-shell']);
    expect(shell.children).toEqual([table]);
    wrapMarkdownTable(table, 0, shell);
    expect(shell.children).toEqual([table]);
  });
});

describe('MarkdownView content safety', () => {
  it('allows web and local links but rejects unsafe/custom schemes', () => {
    for (const url of [
      'https://example.com/docs',
      'HTTP://example.com',
      '#section',
      '/docs/setup',
      '../README.md',
      'images/logo.svg',
    ]) {
      expect(isSafeMarkdownUrl(url), url).toBe(true);
    }
    for (const url of [
      'javascript:alert(1)',
      'java%73cript:alert(1)',
      'data:text/html,<script>alert(1)</script>',
      'file:///etc/passwd',
      'custom-agent://open',
      '//external.example.com/path',
      '\\\\external.example.com/path',
      '%5c%5cexternal.example.com/path',
    ]) {
      expect(isSafeMarkdownUrl(url), url).toBe(false);
    }
  });

  it('removes unsafe link properties, event handlers, and dangerous HTML nodes', () => {
    const unsafeLink = {
      tagName: 'a',
      properties: {
        href: 'javascript:alert(1)',
        onClick: 'alert(1)',
      },
    };
    sanitizeMarkdownNode(unsafeLink);
    expect(unsafeLink.properties).toEqual({});

    const script = { tagName: 'script' };
    const parent = { tagName: 'p', children: [script] };
    sanitizeMarkdownNode(script, 0, parent);
    expect(parent.children).toEqual([]);
  });

  it('removes the preview package raw-HTML parser from the rehype chain', () => {
    function safePlugin() {}
    expect(filterUnsafeMarkdownPlugins('rehype', [rehypeRaw, safePlugin])).toEqual([
      safePlugin,
    ]);
    expect(filterUnsafeMarkdownPlugins('remark', [rehypeRaw])).toEqual([rehypeRaw]);
  });

  it('intercepts fragment, local, and unsafe links on click', () => {
    const scrollIntoView = vi.fn();
    const previousDocument = globalThis.document;
    Object.defineProperty(globalThis, 'document', {
      configurable: true,
      value: {
        getElementById: vi.fn((id: string) =>
          id === 'section' ? { scrollIntoView } : null,
        ),
      },
    });

    try {
      for (const href of ['#section', '/docs/setup', '../README.md', 'javascript:alert(1)']) {
        const event = clickEvent(href);
        handleMarkdownClick(event);
        expect(event.preventDefault).toHaveBeenCalledOnce();
        expect(event.stopPropagation).toHaveBeenCalledOnce();
      }
      expect(scrollIntoView).toHaveBeenCalledWith({ behavior: 'smooth', block: 'start' });
    } finally {
      Object.defineProperty(globalThis, 'document', {
        configurable: true,
        value: previousDocument,
      });
    }
  });

  it('opens http(s) links externally without allowing webview navigation', async () => {
    openExternalLinkMock.mockReset().mockResolvedValue(undefined);
    const event = clickEvent('https://example.com/docs');

    handleMarkdownClick(event);
    await Promise.resolve();

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(event.stopPropagation).toHaveBeenCalledOnce();
    expect(openExternalLinkMock).toHaveBeenCalledWith('https://example.com/docs');
  });

  it('treats file-like markdown hrefs as local paths, not site paths', () => {
    expect(looksLikeMarkdownLocalPath('src/pages/chat/index.tsx')).toBe(true);
    expect(looksLikeMarkdownLocalPath('./README.md')).toBe(true);
    expect(looksLikeMarkdownLocalPath('../docs/guide.md')).toBe(true);
    expect(looksLikeMarkdownLocalPath('/Users/demo/app/src/foo.ts')).toBe(true);
    expect(looksLikeMarkdownLocalPath('README.md')).toBe(true);
    expect(looksLikeMarkdownLocalPath('/docs/setup')).toBe(false);
    expect(isActionableMarkdownHref('/docs/setup')).toBe(false);
    expect(isActionableMarkdownHref('https://example.com')).toBe(true);
  });

  it('resolves relative markdown paths against the working directory', () => {
    expect(resolveMarkdownLocalPath('src/foo.ts', '/Users/demo/app')).toBe(
      '/Users/demo/app/src/foo.ts',
    );
    expect(resolveMarkdownLocalPath('./README.md', 'D:\\demo')).toBe('D:\\demo\\README.md');
    expect(resolveMarkdownLocalPath('src/foo.ts')).toBeNull();
    expect(resolveMarkdownLocalPath('/docs/setup', '/Users/demo/app')).toBeNull();
  });

  it('detects markdown files for in-app preview', () => {
    expect(isMarkdownFilePath('README.md')).toBe(true);
    expect(isMarkdownFilePath('/Users/demo/app/docs/guide.MDX')).toBe(true);
    expect(isMarkdownFilePath('src/pages/chat/index.tsx')).toBe(false);
    expect(localParentDir('/Users/demo/app/README.md')).toBe('/Users/demo/app');
    expect(localParentDir('D:\\demo\\docs\\guide.md')).toBe('D:\\demo\\docs');
  });

  it('lets the chat page handle markdown files instead of the file manager', () => {
    openLocalPathMock.mockReset();
    const onOpenLocal = vi.fn(() => true);
    handleMarkdownClick(clickEvent('README.md'), {
      localBasePath: '/Users/demo/app',
      onOpenLocal,
    });
    expect(onOpenLocal).toHaveBeenCalledWith('/Users/demo/app/README.md');
    expect(openLocalPathMock).not.toHaveBeenCalled();
  });

  it('opens local markdown links in the file manager', async () => {
    openLocalPathMock.mockReset().mockResolvedValue(undefined);
    const event = clickEvent('src/pages/chat/index.tsx');

    handleMarkdownClick(event, { localBasePath: '/Users/demo/app' });
    await Promise.resolve();

    expect(event.preventDefault).toHaveBeenCalledOnce();
    expect(openLocalPathMock).toHaveBeenCalledWith('/Users/demo/app/src/pages/chat/index.tsx');
  });

  it('strips site-style hrefs that cannot be opened', () => {
    const link = { tagName: 'a', properties: { href: '/docs/setup' } };
    sanitizeMarkdownNode(link);
    expect(link.properties).toEqual({});
  });

  it('does not mutate the hash when scrolling a missing or encoded anchor', () => {
    const scrollIntoView = vi.fn();
    const previousDocument = globalThis.document;
    Object.defineProperty(globalThis, 'document', {
      configurable: true,
      value: {
        getElementById: vi.fn((id: string) =>
          id === 'section two' ? { scrollIntoView } : null,
        ),
      },
    });

    try {
      expect(scrollMarkdownAnchor('#section%20two')).toBe(true);
      expect(scrollMarkdownAnchor('#missing')).toBe(false);
      expect(scrollIntoView).toHaveBeenCalledOnce();
    } finally {
      Object.defineProperty(globalThis, 'document', {
        configurable: true,
        value: previousDocument,
      });
    }
  });
});

function clickEvent(href: string) {
  const anchor = {
    getAttribute: (name: string) => (name === 'href' ? href : null),
  };
  return {
    target: {
      closest: (selector: string) => (selector === 'a' ? anchor : null),
    },
    currentTarget: {
      contains: (node: unknown) => node === anchor,
    },
    preventDefault: vi.fn(),
    stopPropagation: vi.fn(),
  } as unknown as Parameters<typeof handleMarkdownClick>[0];
}
