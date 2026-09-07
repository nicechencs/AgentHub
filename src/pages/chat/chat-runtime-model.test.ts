import { describe, expect, it } from 'vitest';
import type { RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';
import {
  acceptsRuntimeSnapshot,
  bindRuntimeSnapshotToAgent,
  canSubmitRuntimeQuestions,
  isLatestRuntimeRead,
  isRuntimeActive,
  isRuntimeChatAgent,
  isRuntimeSessionLocked,
  readRuntimeTransport,
  requestMatchesRuntime,
} from './chat-runtime-model';

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
  it('does not lock an empty Codex chat that is only runtime-eligible', () => {
    expect(isRuntimeSessionLocked(snapshot(true, 'idle'))).toBe(false);
    expect(isRuntimeSessionLocked(snapshot(false, 'idle'))).toBe(false);
    expect(isRuntimeSessionLocked(null)).toBe(false);
    expect(isRuntimeSessionLocked(snapshot(true, 'idle'), { conversationId: 'a' })).toBe(false);
  });
  it('does not lock a new empty chat using another conversation runtime snapshot', () => {
    expect(isRuntimeSessionLocked(snapshot(true, 'completed'), { conversationId: 'b' })).toBe(false);
    expect(isRuntimeSessionLocked(snapshot(true, 'running'), { conversationId: 'new' })).toBe(false);
  });
  it('locks once this conversation has a message or a started session', () => {
    expect(isRuntimeSessionLocked(null, { hasMessages: true })).toBe(true);
    expect(isRuntimeSessionLocked(snapshot(false, 'idle'), { nativeSessionId: 'thread-1' })).toBe(true);
    expect(isRuntimeSessionLocked(snapshot(true, 'running'))).toBe(true);
    expect(isRuntimeSessionLocked(snapshot(true, 'completed'))).toBe(true);
    expect(isRuntimeSessionLocked({ ...snapshot(true, 'idle'), runId: 'run-a' })).toBe(true);
    expect(isRuntimeSessionLocked(snapshot(true, 'idle'), { nativeSessionId: 'thread-1' })).toBe(true);
    expect(isRuntimeSessionLocked(snapshot(true, 'completed'), { conversationId: 'a' })).toBe(true);
  });
  it('treats Codex, Grok, and Kiro as continuous-chat agents', () => {
    expect(isRuntimeChatAgent('codex')).toBe(true);
    expect(isRuntimeChatAgent('grok')).toBe(true);
    expect(isRuntimeChatAgent('pi')).toBe(false);
    expect(isRuntimeChatAgent('claude')).toBe(false);
    expect(isRuntimeChatAgent('cursor')).toBe(false);
    expect(isRuntimeChatAgent('kiro')).toBe(true);
    expect(isRuntimeChatAgent(null)).toBe(false);
  });
  it('drops leftover enabled snapshot when the conversation is no longer a continuous-chat agent', () => {
    const leftover = snapshot(true, 'idle');
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'pi', conversationId: 'a' })?.enabled).toBe(false);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'kiro', conversationId: 'a' })?.enabled).toBe(true);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'cursor', conversationId: 'a' })?.enabled).toBe(false);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'codex', conversationId: 'a' })?.enabled).toBe(true);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'grok', conversationId: 'a' })?.enabled).toBe(true);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'pi', conversationId: 'b' })).toBeNull();
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'codex' })).toBeNull();
  });
  it('requires every runtime question to have an answer before submit', () => {
    const request: Pick<RuntimeRequest, 'kind' | 'questions'> = {
      kind: 'question', questions: [{ id: 'q', header: '', question: '', options: [], isOther: false, isSecret: false }],
    };
    expect(canSubmitRuntimeQuestions(request, {})).toBe(false);
    expect(canSubmitRuntimeQuestions(request, { q: ['freeform'] })).toBe(true);
  });
});
