import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { ChatQueuedFollowUpList } from './ChatQueuedFollowUpList';

vi.mock('@/components/shared/LanguageProvider', async () => {
  const { createTranslator } = await import('@/lib/i18n');
  const t = createTranslator('zh');
  return {
    useI18n: () => ({ lang: 'zh', setLanguage: () => undefined, t }),
  };
});

function renderMarkup(node: ReactElement) {
  return renderToStaticMarkup(node);
}

describe('queued follow-up list', () => {
  it('shows each queued line as its own cancelable row', () => {
    const html = renderMarkup(
      createElement(ChatQueuedFollowUpList, {
        items: [
          { id: 'q-2', text: '第二条' },
          { id: 'q-3', text: '第三条' },
        ],
        onCancelItem: () => undefined,
        onCancelAll: () => undefined,
      }),
    );
    expect(html).toContain('已排队 2 条');
    expect(html).toContain('本轮结束后发送');
    expect(html).toContain('第二条');
    expect(html).toContain('第三条');
    expect(html).not.toContain('第二条；第三条');
    expect(html).toContain('全部取消');
    expect(html.match(/取消这条/g)?.length).toBe(2);
    expect(html).toContain('data-help="chat-queued-follow-ups"');
  });

  it('hides an empty queue', () => {
    const html = renderMarkup(
      createElement(ChatQueuedFollowUpList, {
        items: [{ id: 'blank', text: '  ' }],
      }),
    );
    expect(html).toBe('');
  });
});
