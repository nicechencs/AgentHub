import { describe, expect, it } from 'vitest';
import rehypeRaw from 'rehype-raw';
import {
  filterUnsafeMarkdownPlugins,
  isSafeMarkdownUrl,
  sanitizeMarkdownNode,
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
});
