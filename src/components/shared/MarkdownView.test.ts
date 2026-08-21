import { describe, expect, it, vi } from 'vitest';
import rehypeRaw from 'rehype-raw';

const { openExternalLinkMock } = vi.hoisted(() => ({
  openExternalLinkMock: vi.fn(),
}));

vi.mock('@/lib/open-external', () => ({
  isHttpUrl: (url: string) => /^https?:\/\//i.test(url.trim()),
  openExternalLink: openExternalLinkMock,
}));

import {
  filterUnsafeMarkdownPlugins,
  handleMarkdownClick,
  isSafeMarkdownUrl,
  sanitizeMarkdownNode,
  scrollMarkdownAnchor,
} from './MarkdownView';

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
