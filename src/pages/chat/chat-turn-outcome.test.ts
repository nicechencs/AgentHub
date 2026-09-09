import { describe, expect, it } from 'vitest';
import type { ChatMessage } from '@/lib/types';
import type { TurnGroup } from './chat-format';
import { lastTurnOutcome, turnOutcomeDetail } from './chat-turn-outcome';

function msg(partial: Partial<ChatMessage> & Pick<ChatMessage, 'id' | 'role' | 'status'>): ChatMessage {
  return {
    conversationId: 'c1',
    turn: 1,
    content: '',
    durationMs: 0,
    createdAt: '2026-01-01T00:00:00.000Z',
    ...partial,
  };
}

describe('lastTurnOutcome', () => {
  it('returns null while sending or when last turn succeeded', () => {
    const turns: TurnGroup[] = [
      {
        turn: 1,
        user: msg({ id: 'u1', role: 'user', status: 'ok', content: 'hi' }),
        agents: [msg({ id: 'a1', role: 'agent', status: 'ok', content: 'all tests passed' })],
      },
    ];
    expect(lastTurnOutcome(turns, true)).toBeNull();
    expect(lastTurnOutcome(turns, false)).toBeNull();
  });

  it('maps cancelled+interrupted error without trusting model prose', () => {
    const turns: TurnGroup[] = [
      {
        turn: 1,
        user: msg({ id: 'u1', role: 'user', status: 'ok', content: 'do work' }),
        agents: [
          msg({
            id: 'a1',
            role: 'agent',
            status: 'cancelled',
            content: 'tests passed',
            error: 'runtime interrupted',
          }),
        ],
      },
    ];
    const outcome = lastTurnOutcome(turns, false);
    expect(outcome?.kind).toBe('interrupted');
    expect(outcome?.source).toBe('message-status');
    expect(outcome?.prompt).toBe('do work');
  });

  it('maps failed status for retry recovery', () => {
    const turns: TurnGroup[] = [
      {
        turn: 2,
        user: msg({ id: 'u2', role: 'user', status: 'ok', content: 'retry me', turn: 2 }),
        agents: [msg({ id: 'a2', role: 'agent', status: 'failed', content: '', turn: 2, error: 'boom' })],
      },
    ];
    expect(lastTurnOutcome(turns, false)?.kind).toBe('failed');
  });

  it('does not show a raw cancelled status word as the stop reason', () => {
    const hint = '已按你的要求停止。可恢复草稿后重发。';
    expect(
      turnOutcomeDetail({ kind: 'cancelled', errorText: 'cancelled' }, (text) => text, hint),
    ).toBe(hint);
    expect(
      turnOutcomeDetail({ kind: 'cancelled', errorText: null }, (text) => text, hint),
    ).toBe(hint);
    expect(
      turnOutcomeDetail(
        { kind: 'failed', errorText: 'boom' },
        (text) => `localized:${text}`,
        hint,
      ),
    ).toBe('localized:boom');
  });
});
