import { describe, expect, it } from 'vitest';
import { createTranslator, translate } from '@/lib/i18n';
import {
  composerCapabilityHint,
  composerCompactSecondary,
  composerConnectionTooltip,
  composerHoverHint,
  composerInvitePlaceholder,
  composerShowsHintRow,
  composerShowsQueueOnlyHint,
  emptyStarterChipHint,
  emptyTranscriptCopy,
} from './chat-empty-state';

const zh = createTranslator('zh');
const en = createTranslator('en');

describe('empty transcript copy', () => {
  it('keeps the headline and chip draft-only hint, without a second Agent name', () => {
    expect(emptyTranscriptCopy(zh)).toEqual({
      headline: '开始对话',
      startersHint: '示例只填入输入框，由你发送',
    });
    expect(emptyTranscriptCopy(en)).toEqual({
      headline: 'Start chatting',
      startersHint: 'Examples fill the box only; you send',
    });
    expect(emptyStarterChipHint('看目录结构和主要功能', '示例只填入输入框，由你发送')).toBe(
      '看目录结构和主要功能 · 示例只填入输入框，由你发送',
    );
  });
});

describe('composer invite and queue-only hint', () => {
  it('uses a generic placeholder on an empty transcript', () => {
    expect(composerInvitePlaceholder(zh, { emptyTranscript: true })).toBe('发消息…');
    expect(composerInvitePlaceholder(en, { emptyTranscript: true })).toBe('Send a message…');
    expect(composerInvitePlaceholder(zh, { emptyTranscript: false })).toBe('发给 Agent…');
    expect(translate('zh', 'chat.composer.placeholderInvite')).not.toContain('{agent}');
  });

  it('keeps capability limits off the placeholder and on a hover title', () => {
    expect(composerShowsQueueOnlyHint('kiro')).toBe(true);
    expect(composerShowsQueueOnlyHint('grok')).toBe(true);
    expect(composerShowsQueueOnlyHint('claude')).toBe(true);
    expect(composerShowsQueueOnlyHint('codex')).toBe(false);
    expect(composerShowsQueueOnlyHint('cursor')).toBe(false);
    expect(composerCapabilityHint(zh, { agentId: 'kiro', sending: false })).toBe(
      '生成时不能中途补充，可排队到下一轮。',
    );
    expect(composerCapabilityHint(en, { agentId: 'kiro', sending: false })).toBe(
      "You can't add more mid-run; you can queue for the next turn.",
    );
    expect(composerCapabilityHint(zh, { agentId: 'kiro', sending: true })).toBeNull();
    expect(composerCapabilityHint(zh, { agentId: 'codex', sending: false })).toBeNull();
    expect(
      composerHoverHint('Enter 发送 · Shift+Enter 换行', '生成时不能中途补充，可排队到下一轮。'),
    ).toContain('Enter 发送');
    expect(
      composerHoverHint('Enter 发送 · Shift+Enter 换行', '生成时不能中途补充，可排队到下一轮。'),
    ).toContain('不能中途补充');
    expect(translate('zh', 'chat.kiro.placeholder')).toBe('发给 Agent…');
  });

  it('hides the permanent shortcut row only on an empty transcript', () => {
    expect(composerShowsHintRow({ emptyTranscript: true })).toBe(false);
    expect(composerShowsHintRow({ emptyTranscript: false })).toBe(true);
    expect(composerCompactSecondary({ emptyTranscript: true })).toBe(true);
    expect(composerCompactSecondary({ emptyTranscript: false })).toBe(false);
  });

  it('builds a quiet connection tooltip from the full label', () => {
    expect(
      composerConnectionTooltip({
        label: 'cunsen.chen@example.com',
        subtitle: '/tmp/demo',
        caption: null,
      }),
    ).toBe('cunsen.chen@example.com · /tmp/demo');
    expect(
      composerConnectionTooltip({
        label: '本机',
        subtitle: null,
        caption: 'Claude · 切换连接',
      }),
    ).toBe('本机 · Claude · 切换连接');
  });
});
