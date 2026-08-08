import { describe, expect, it } from 'vitest';
import {
  hasProcessDetails,
  phaseFromMessageStatus,
  processKey,
  processPhaseLabel,
  reduceProcessEvent,
  stepSummary,
  type ProcessMap,
} from '@/lib/chat-process';
import type { ChatEvent, ChatMessage } from '@/lib/types';

function finishedMsg(partial: Partial<ChatMessage> & Pick<ChatMessage, 'status' | 'content'>): ChatMessage {
  return {
    id: 'm1',
    conversationId: 'c1',
    turn: 1,
    role: 'agent',
    agentId: 'claude',
    durationMs: 10,
    createdAt: 't',
    ...partial,
  };
}

describe('chat-process reduceProcessEvent', () => {
  it('processKey joins turn and agent', () => {
    expect(processKey(2, 'codex')).toBe('2:codex');
  });

  it('started seeds queued agents', () => {
    const ev: ChatEvent = { type: 'started', turn: 1, agents: ['claude', 'codex'] };
    const map = reduceProcessEvent({}, ev, 1000);
    expect(map['1:claude']?.phase).toBe('queued');
    expect(map['1:codex']?.phase).toBe('queued');
    expect(map['1:claude']?.command).toBeUndefined();
    expect(map['1:claude']?.steps).toEqual([]);
  });

  it('agentStarted records command and running', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'started', turn: 1, agents: ['claude'] },
      1,
    );
    map = reduceProcessEvent(
      map,
      { type: 'agentStarted', turn: 1, agent: 'claude', command: 'claude -p hi' },
      2,
    );
    expect(map['1:claude']).toMatchObject({
      phase: 'running',
      command: 'claude -p hi',
    });
  });

  it('stdout and stderr accumulate separately', () => {
    let map: ProcessMap = {};
    map = reduceProcessEvent(
      map,
      { type: 'agentChunk', turn: 1, agent: 'kimi', stream: 'stdout', text: 'A' },
      1,
    );
    map = reduceProcessEvent(
      map,
      { type: 'agentChunk', turn: 1, agent: 'kimi', stream: 'stderr', text: 'e1\n' },
      2,
    );
    map = reduceProcessEvent(
      map,
      { type: 'agentChunk', turn: 1, agent: 'kimi', stream: 'stdout', text: 'B' },
      3,
    );
    expect(map['1:kimi']?.stdout).toBe('AB');
    expect(map['1:kimi']?.stderr).toBe('e1\n');
    expect(map['1:kimi']?.phase).toBe('running');
  });

  it('agentProcess records tool steps but skips text steps in timeline', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'claude', command: 'x' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'claude',
        step: { type: 'tool', name: 'Read', status: 'start', id: 't1' },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'claude',
        step: { type: 'text', text: 'hello' },
      },
      3,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'claude',
        step: { type: 'thinking', text: 'hmm', done: false },
      },
      4,
    );
    expect(map['1:claude']?.steps).toHaveLength(2);
    expect(map['1:claude']?.steps[0]).toMatchObject({ type: 'tool', name: 'Read' });
    expect(map['1:claude']?.steps[1]).toMatchObject({ type: 'thinking' });
    expect(stepSummary(map['1:claude']!.steps[0])).toContain('Read');
  });

  it('agentFinished maps status to phase', () => {
    let map = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'claude', command: 'x' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentFinished',
        turn: 1,
        agent: 'claude',
        message: finishedMsg({ status: 'ok', content: 'done' }),
      },
      2,
    );
    expect(map['1:claude']?.phase).toBe('ok');
    expect(map['1:claude']?.stdout).toBe('done');
    expect(map['1:claude']?.command).toBe('x');
  });

  it('phaseFromMessageStatus and labels', () => {
    expect(phaseFromMessageStatus('cancelled')).toBe('cancelled');
    expect(phaseFromMessageStatus('failed')).toBe('failed');
    expect(processPhaseLabel('queued')).toBe('排队中');
    expect(processPhaseLabel('running')).toBe('生成中');
  });

  it('hasProcessDetails is true for command, steps, or active phases', () => {
    expect(hasProcessDetails(undefined)).toBe(false);
    expect(
      hasProcessDetails({
        turn: 1,
        agent: 'claude',
        phase: 'ok',
        stdout: 'hi',
        stderr: '',
        steps: [],
        updatedAt: 0,
      }),
    ).toBe(false);
    expect(
      hasProcessDetails({
        turn: 1,
        agent: 'claude',
        phase: 'ok',
        command: 'claude -p',
        stdout: 'hi',
        stderr: '',
        steps: [],
        updatedAt: 0,
      }),
    ).toBe(true);
    expect(
      hasProcessDetails({
        turn: 1,
        agent: 'claude',
        phase: 'ok',
        stdout: '',
        stderr: '',
        steps: [{ type: 'tool', name: 'Bash', status: 'end' }],
        updatedAt: 0,
      }),
    ).toBe(true);
    expect(
      hasProcessDetails({
        turn: 1,
        agent: 'claude',
        phase: 'running',
        stdout: '',
        stderr: '',
        steps: [],
        updatedAt: 0,
      }),
    ).toBe(true);
  });

  it('ignores finished/error events without mutating identity when empty', () => {
    const empty: ProcessMap = {};
    const a = reduceProcessEvent(empty, { type: 'finished', turn: 1, ok: true });
    expect(a).toBe(empty);
    const b = reduceProcessEvent(empty, { type: 'error', message: 'x' });
    expect(b).toBe(empty);
  });

  it('finished finalizes still-active process views for the turn', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'started', turn: 2, agents: ['claude', 'codex'] },
      1,
    );
    map = reduceProcessEvent(
      map,
      { type: 'agentStarted', turn: 2, agent: 'claude', command: 'claude -p' },
      2,
    );
    map = reduceProcessEvent(map, { type: 'finished', turn: 2, ok: true }, 3);
    expect(map['2:claude']?.phase).toBe('ok');
    expect(map['2:codex']?.phase).toBe('ok');
  });
});
