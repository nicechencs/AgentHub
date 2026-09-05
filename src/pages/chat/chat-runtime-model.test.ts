import { describe, expect, it } from 'vitest';
import type { RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';
import { acceptsRuntimeSnapshot, canSubmitRuntimeQuestions, isLatestRuntimeRead, isRuntimeActive, readRuntimeTransport, requestMatchesRuntime } from './chat-runtime-model';

const snapshot = (enabled: boolean, phase: RuntimeSnapshot['phase'] = 'idle'): RuntimeSnapshot => ({
  conversationId: 'a', enabled, runId: phase === 'idle' ? null : 'run-a', phase,
  lastSequence: 0, events: [], pendingRequests: [], gap: false,
});

describe('chat runtime transport guards', () => {
  it('does not allow a failed snapshot read to use legacy send', async () => {
    await expect(readRuntimeTransport(async () => { throw new Error('offline'); })).resolves.toEqual({ kind: 'unavailable' });
  });
  it('keeps runtime and legacy selection explicit', async () => {
    await expect(readRuntimeTransport(async () => snapshot(true))).resolves.toMatchObject({ kind: 'runtime' });
    await expect(readRuntimeTransport(async () => snapshot(false))).resolves.toMatchObject({ kind: 'legacy' });
  });
  it('rejects an A snapshot that arrives after A → B → A', () => {
    expect(acceptsRuntimeSnapshot('a', 3, 'a', 2)).toBe(false);
    expect(acceptsRuntimeSnapshot('a', 3, 'a', 3)).toBe(true);
  });
  it('drops an older poll response after a newer read started', () => {
    expect(isLatestRuntimeRead(4, 5)).toBe(false);
    expect(isLatestRuntimeRead(5, 5)).toBe(true);
  });
  it('rejects a late request from a prior run', () => {
    const request: RuntimeRequest = { id: 'request-a', runId: 'run-old', kind: 'command', title: '', detail: '', questions: [] };
    expect(requestMatchesRuntime(request, 'run-new')).toBe(false);
    expect(requestMatchesRuntime(request, 'run-old')).toBe(true);
  });
  it('keeps cancelling active until a terminal snapshot arrives', () => {
    expect(isRuntimeActive('cancelling')).toBe(true);
    expect(isRuntimeActive('cancelled')).toBe(false);
  });
  it('requires every runtime question to have an answer before submit', () => {
    const request: Pick<RuntimeRequest, 'kind' | 'questions'> = {
      kind: 'question', questions: [{ id: 'q', header: '', question: '', options: [], isOther: false, isSecret: false }],
    };
    expect(canSubmitRuntimeQuestions(request, {})).toBe(false);
    expect(canSubmitRuntimeQuestions(request, { q: ['freeform'] })).toBe(true);
  });
});
