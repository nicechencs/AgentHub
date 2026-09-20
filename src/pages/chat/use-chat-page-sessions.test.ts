import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import type { Conversation } from '@/lib/types';
import { mergeHandoffConversations } from './use-chat-page-sessions';

function conv(id: string, cwd?: string): Conversation {
  return {
    id,
    title: id,
    agentIds: ['claude'],
    allowDangerous: false,
    createdAt: '',
    updatedAt: '',
    cwd,
  };
}

describe('mergeHandoffConversations', () => {
  it('keeps a folder session prepended before a stale empty list load commits', () => {
    const folder = conv('folder', 'D:\\work\\app');
    expect(mergeHandoffConversations([folder], [])).toEqual([folder]);
  });

  it('keeps the handoff session in front of a stale list that missed it', () => {
    const folder = conv('folder', 'D:\\work\\app');
    const existing = conv('existing');
    expect(mergeHandoffConversations([folder], [existing])).toEqual([folder, existing]);
  });

  it('returns the loaded list when the previous rail is empty', () => {
    const loaded = [conv('a'), conv('b')];
    expect(mergeHandoffConversations([], loaded)).toBe(loaded);
  });

  it('returns the loaded list identity when the handoff session is already present', () => {
    const folder = conv('folder', 'D:\\work\\app');
    const existing = conv('existing');
    const loaded = [folder, existing];
    expect(mergeHandoffConversations([folder], loaded)).toBe(loaded);
  });
});

describe('handleNewChat cwd', () => {
  it('sanitizes the new-chat folder before createConversation', () => {
    const src = readFileSync(new URL('./use-chat-page-sessions.ts', import.meta.url), 'utf8');
    expect(src).toContain('newChatCwdArg');
    expect(src).toContain('cwd === undefined ? defaults.cwd : cwd');
    expect(src).not.toContain('cwdOverride === undefined ? defaults.cwd : cwdOverride');
  });
});
