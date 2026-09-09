import { describe, expect, it } from 'vitest';
import { createTranslator, translate } from '@/lib/i18n';
import {
  composerCapabilityHint,
  composerCompactSecondary,
  composerInvitePlaceholder,
  composerShowsQueueOnlyHint,
  emptyTranscriptCopy,
} from './chat-empty-state';

const zh = createTranslator('zh');
const en = createTranslator('en');

describe('empty transcript copy', () => {
  it('invites typing first and keeps chips as draft-only', () => {
    expect(emptyTranscriptCopy(zh, { agentLabel: 'Claude', projectLabel: 'demo' })).toEqual({
      headline: '开始对话',
      invite: '向 Claude 发送第一条消息',
      identity: 'Claude · demo',
      startersHint: '示例只填入输入框，由你发送',
    });
    expect(emptyTranscriptCopy(en, { agentLabel: 'Claude', projectLabel: 'demo' })).toEqual({
      headline: 'Start chatting',
      invite: 'Send the first message to Claude',
      identity: 'Claude · demo',
      startersHint: 'Examples fill the box only; you send',
    });
  });
});

describe('composer invite and queue-only hint', () => {
  it('uses an invite placeholder on an empty transcript', () => {
    expect(
      composerInvitePlaceholder(zh, { emptyTranscript: true, agentLabel: 'Kiro' }),
    ).toBe('向 Kiro 发第一条消息…');
    expect(
      composerInvitePlaceholder(en, { emptyTranscript: true, agentLabel: 'Kiro' }),
    ).toBe('Send the first message to Kiro…');
    expect(
      composerInvitePlaceholder(zh, { emptyTranscript: false, agentLabel: 'Kiro' }),
    ).toBe('发给 Agent…');
    expect(
      composerInvitePlaceholder(zh, { emptyTranscript: true, agentLabel: '  ' }),
    ).toBe('发给 Agent…');
  });

  it('keeps capability limits off the placeholder and on a quiet hint', () => {
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
    expect(translate('zh', 'chat.composer.placeholderInvite', { agent: 'Kiro' })).not.toContain(
      '不能中途补充',
    );
    expect(translate('zh', 'chat.kiro.placeholder')).toBe('发给 Agent…');
  });

  it('compacts secondary toolbar labels only on an empty transcript', () => {
    expect(composerCompactSecondary({ emptyTranscript: true })).toBe(true);
    expect(composerCompactSecondary({ emptyTranscript: false })).toBe(false);
  });
});
