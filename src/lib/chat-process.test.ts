import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  hasProcessDetails,
  mergeThinkingText,
  phaseFromMessageStatus,
  processKey,
  processPhaseLabel,
  reduceProcessEvent,
  stepSummary,
  type ProcessMap,
} from '@/lib/chat-process';
import type { ChatEvent, ChatMessage } from '@/lib/types';

const t = createTranslator('zh');

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
    expect(stepSummary(map['1:claude']!.steps[0], t)).toContain('Read');
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

  it('maps raw step notes to the translated label (zh)', () => {
    expect(
      stepSummary({ type: 'raw', text: '{…}', note: 'unrecognized structured line' }, t),
    ).toBe('无法识别的输出行');
    expect(
      stepSummary({ type: 'raw', text: 'oops', note: 'non-json line in structured mode' }, t),
    ).toBe('结构化模式下出现非 JSON 行');
    expect(stepSummary({ type: 'raw', text: 'x', note: 'line too long' }, t)).toBe('输出行过长');
    expect(stepSummary({ type: 'raw', text: '{…}', note: '无法识别的输出行' }, t)).toBe(
      '无法识别的输出行',
    );
  });

  it('maps raw step notes to the translated label (en)', () => {
    const tEn = createTranslator('en');
    expect(
      stepSummary({ type: 'raw', text: '{…}', note: 'unrecognized structured line' }, tEn),
    ).toBe('Unrecognized output line');
    expect(
      stepSummary({ type: 'raw', text: 'oops', note: 'non-json line in structured mode' }, tEn),
    ).toBe('Non-JSON line in structured mode');
    expect(stepSummary({ type: 'raw', text: 'x', note: 'line too long' }, tEn)).toBe(
      'Output line too long',
    );
    expect(stepSummary({ type: 'raw', text: '{…}', note: '无法识别的输出行' }, tEn)).toBe(
      'Unrecognized output line',
    );
  });

  it('phaseFromMessageStatus and labels', () => {
    expect(phaseFromMessageStatus('cancelled')).toBe('cancelled');
    expect(phaseFromMessageStatus('failed')).toBe('failed');
    expect(processPhaseLabel('queued', t)).toBe('排队中');
    expect(processPhaseLabel('running', t)).toBe('生成中');
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

  it('merges consecutive thinking deltas and marks done when a tool starts', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'grok', command: 'grok -p' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'thinking', text: 'Hel', done: false },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'thinking', text: 'lo', done: false },
      },
      3,
    );
    expect(map['1:grok']?.steps).toHaveLength(1);
    expect(map['1:grok']?.steps[0]).toMatchObject({ type: 'thinking', text: 'Hello', done: false });

    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'tool', id: 't1', name: 'Read', status: 'start' },
      },
      4,
    );
    expect(map['1:grok']?.steps).toHaveLength(2);
    expect(map['1:grok']?.steps[0]).toMatchObject({ type: 'thinking', done: true });
    expect(map['1:grok']?.steps[1]).toMatchObject({ type: 'tool', id: 't1', status: 'start' });
  });

  it('mergeThinkingText treats Codex snapshots as replace and Grok/Pi as append', () => {
    expect(mergeThinkingText('Hel', 'lo')).toBe('Hello');
    expect(mergeThinkingText('Hello', 'Hello world')).toBe('Hello world');
    expect(mergeThinkingText('Hello', 'Hello')).toBe('Hello');
    expect(mergeThinkingText('Hello world', 'Hello')).toBe('Hello world');
    expect(mergeThinkingText('plan', '')).toBe('plan');
  });

  it('replaces Codex-style full thinking snapshots instead of concatenating', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'codex', command: 'codex exec' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'Hello', done: false },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'Hello world', done: false },
      },
      3,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'Hello world', done: true },
      },
      4,
    );
    expect(map['1:codex']?.steps).toHaveLength(1);
    expect(map['1:codex']?.steps[0]).toMatchObject({
      type: 'thinking',
      text: 'Hello world',
      done: true,
    });
  });

  it('does not merge a later thinking snapshot onto an already-done step', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'codex', command: 'codex exec' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'item1 complete thought', done: true },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'item2 unrelated snapshot', done: false },
      },
      3,
    );
    expect(map['1:codex']?.steps).toHaveLength(2);
    expect(map['1:codex']?.steps[0]).toMatchObject({
      type: 'thinking',
      text: 'item1 complete thought',
      done: true,
    });
    expect(map['1:codex']?.steps[1]).toMatchObject({
      type: 'thinking',
      text: 'item2 unrelated snapshot',
      done: false,
    });
  });

  it('still merges consecutive undone snapshots after a done step', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'codex', command: 'codex exec' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'item1 done', done: true },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'a', done: false },
      },
      3,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'codex',
        step: { type: 'thinking', text: 'a b', done: false },
      },
      4,
    );
    expect(map['1:codex']?.steps).toHaveLength(2);
    expect(map['1:codex']?.steps[0]).toMatchObject({
      type: 'thinking',
      text: 'item1 done',
      done: true,
    });
    expect(map['1:codex']?.steps[1]).toMatchObject({
      type: 'thinking',
      text: 'a b',
      done: false,
    });
  });

  it('merges tool updates by id including later parallel tools', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'grok', command: 'grok -p' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'tool', id: 't1', name: 'Read', status: 'start', input: { path: 'a.rs' } },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'tool', id: 't2', name: 'Bash', status: 'start' },
      },
      3,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: {
          type: 'tool',
          id: 't1',
          name: 'Read',
          status: 'end',
          result: 'fn main() {}',
        },
      },
      4,
    );
    expect(map['1:grok']?.steps).toHaveLength(2);
    expect(map['1:grok']?.steps[0]).toMatchObject({
      type: 'tool',
      id: 't1',
      status: 'end',
      result: 'fn main() {}',
    });
    expect(map['1:grok']?.steps[1]).toMatchObject({ type: 'tool', id: 't2', name: 'Bash' });
  });

  it('starts a new thinking episode after a tool', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'grok', command: 'x' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'thinking', text: 'first', done: false },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'tool', id: 't1', name: 'Read', status: 'start' },
      },
      3,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'thinking', text: 'second', done: false },
      },
      4,
    );
    expect(map['1:grok']?.steps).toHaveLength(3);
    expect(map['1:grok']?.steps[0]).toMatchObject({ type: 'thinking', text: 'first', done: true });
    expect(map['1:grok']?.steps[2]).toMatchObject({ type: 'thinking', text: 'second', done: false });
  });

  it('appends tools without id instead of merging', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'grok', command: 'x' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'tool', name: 'Bash', status: 'start' },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'tool', name: 'Bash', status: 'end', result: 'ok' },
      },
      3,
    );
    expect(map['1:grok']?.steps).toHaveLength(2);
    expect(map['1:grok']?.steps[0]).toMatchObject({ type: 'tool', status: 'start' });
    expect(map['1:grok']?.steps[1]).toMatchObject({ type: 'tool', status: 'end', result: 'ok' });
  });

  it('agentFinished marks leftover thinking done', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'grok', command: 'x' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 1,
        agent: 'grok',
        step: { type: 'thinking', text: 'hmm', done: false },
      },
      2,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentFinished',
        turn: 1,
        agent: 'grok',
        message: finishedMsg({ status: 'ok', content: 'done', agentId: 'grok' }),
      },
      3,
    );
    expect(map['1:grok']?.steps[0]).toMatchObject({ type: 'thinking', done: true });
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

  it('finished marks leftover thinking done for the turn', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 3, agent: 'grok', command: 'x' },
      1,
    );
    map = reduceProcessEvent(
      map,
      {
        type: 'agentProcess',
        turn: 3,
        agent: 'grok',
        step: { type: 'thinking', text: 'still', done: false },
      },
      2,
    );
    map = reduceProcessEvent(map, { type: 'finished', turn: 3, ok: true }, 3);
    expect(map['3:grok']?.phase).toBe('ok');
    expect(map['3:grok']?.steps[0]).toMatchObject({ type: 'thinking', text: 'still', done: true });
  });

  it('finished with cancelled stays cancelled even when ok is true', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'started', turn: 4, agents: ['claude'] },
      1,
    );
    map = reduceProcessEvent(
      map,
      { type: 'agentStarted', turn: 4, agent: 'claude', command: 'x' },
      2,
    );
    map = reduceProcessEvent(
      map,
      { type: 'finished', turn: 4, ok: true, cancelled: true },
      3,
    );
    expect(map['4:claude']?.phase).toBe('cancelled');
  });

  it('finished ok:false marks still-active views failed', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'started', turn: 5, agents: ['claude'] },
      1,
    );
    map = reduceProcessEvent(
      map,
      { type: 'agentStarted', turn: 5, agent: 'claude', command: 'x' },
      2,
    );
    map = reduceProcessEvent(map, { type: 'finished', turn: 5, ok: false }, 3);
    expect(map['5:claude']?.phase).toBe('failed');
  });

  it('caps soft steps first and keeps early tools', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'claude', command: 'x' },
      1,
    );
    let tick = 2;
    for (const id of ['t1', 't2', 't3']) {
      map = reduceProcessEvent(
        map,
        {
          type: 'agentProcess',
          turn: 1,
          agent: 'claude',
          step: { type: 'tool', id, name: 'Read', status: 'start' },
        },
        tick,
      );
      tick += 1;
    }
    for (let i = 0; i < 201; i += 1) {
      map = reduceProcessEvent(
        map,
        {
          type: 'agentProcess',
          turn: 1,
          agent: 'claude',
          step: { type: 'thinking', text: `think-${i}`, done: true },
        },
        tick,
      );
      tick += 1;
    }
    const steps = map['1:claude']?.steps ?? [];
    expect(steps).toHaveLength(200);
    expect(steps.filter((s) => s.type === 'tool')).toEqual([
      expect.objectContaining({ type: 'tool', id: 't1' }),
      expect.objectContaining({ type: 'tool', id: 't2' }),
      expect.objectContaining({ type: 'tool', id: 't3' }),
    ]);
    expect(steps.filter((s) => s.type === 'thinking').length).toBeLessThan(201);
    expect(steps.some((s) => s.type === 'thinking' && s.text === 'think-0')).toBe(false);
  });

  it('caps overflowing tools to the newest MAX_STEPS and drops the oldest', () => {
    let map: ProcessMap = reduceProcessEvent(
      {},
      { type: 'agentStarted', turn: 1, agent: 'claude', command: 'x' },
      1,
    );
    for (let i = 0; i < 201; i += 1) {
      map = reduceProcessEvent(
        map,
        {
          type: 'agentProcess',
          turn: 1,
          agent: 'claude',
          step: { type: 'tool', id: `tool-${i}`, name: 'Read', status: 'start' },
        },
        i + 2,
      );
    }
    const steps = map['1:claude']?.steps ?? [];
    expect(steps).toHaveLength(200);
    expect(steps[0]).toMatchObject({ type: 'tool', id: 'tool-1' });
    expect(steps[199]).toMatchObject({ type: 'tool', id: 'tool-200' });
    expect(steps.some((s) => s.type === 'tool' && s.id === 'tool-0')).toBe(false);
  });
});
