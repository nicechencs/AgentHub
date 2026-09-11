import { describe, expect, it } from 'vitest';
import type { RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';
import { translate, type TranslateFn } from '@/lib/i18n';
import {
  acceptsRuntimeSnapshot,
  bindRuntimeSnapshotToAgent,
  canSubmitRuntimeQuestions,
  fileChangePreviewHintKey,
  runtimeFileChangePreview,
  runtimeReplyFields,
  requestAllowsAlways,
  runtimeAllowAlwaysCopy,
  runtimeAllowAlwaysHintKey,
  runtimeRequestTitle,
  isLatestRuntimeRead,
  isRuntimeActive,
  isRuntimeChatAgent,
  isRuntimeSessionLocked,
  nativeCommandMenuEnabled,
  shouldRefreshRuntimeCatalog,
  readRuntimeTransport,
  requestMatchesRuntime,
  runtimePlanEntryTone,
  visibleRuntimePlan,
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
  it('treats Codex, Grok, Kiro, and Claude as continuous-chat agents', () => {
    expect(isRuntimeChatAgent('codex')).toBe(true);
    expect(isRuntimeChatAgent('grok')).toBe(true);
    expect(isRuntimeChatAgent('pi')).toBe(false);
    expect(isRuntimeChatAgent('claude')).toBe(true);
    expect(isRuntimeChatAgent('cursor')).toBe(false);
    expect(isRuntimeChatAgent('kiro')).toBe(true);
    expect(isRuntimeChatAgent(null)).toBe(false);
  });
  it('refreshes the Options catalog only when the snapshot epoch increases', () => {
    expect(shouldRefreshRuntimeCatalog(0, 0)).toBe(false);
    expect(shouldRefreshRuntimeCatalog(2, 2)).toBe(false);
    expect(shouldRefreshRuntimeCatalog(2, 1)).toBe(false);
    expect(shouldRefreshRuntimeCatalog(0, 1)).toBe(true);
  });
  it('lists native slash commands only when the session catalog is ready', () => {
    expect(nativeCommandMenuEnabled({ sessionReady: false, nativeCommands: [{ name: 'compact' }] })).toBe(false);
    expect(nativeCommandMenuEnabled({ sessionReady: true, nativeCommands: [] })).toBe(false);
    expect(nativeCommandMenuEnabled({ sessionReady: true, nativeCommands: [{ name: 'compact' }] })).toBe(true);
  });
  it('drops leftover enabled snapshot when the conversation is no longer a continuous-chat agent', () => {
    const leftover = snapshot(true, 'idle');
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'pi', conversationId: 'a' })?.enabled).toBe(false);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'kiro', conversationId: 'a' })?.enabled).toBe(true);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'cursor', conversationId: 'a' })?.enabled).toBe(false);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'codex', conversationId: 'a' })?.enabled).toBe(true);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'grok', conversationId: 'a' })?.enabled).toBe(true);
    expect(bindRuntimeSnapshotToAgent(leftover, { agentId: 'claude', conversationId: 'a' })?.enabled).toBe(true);
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
  it('omits answers when allowing or denying, and omits decision for questions', () => {
    expect(runtimeReplyFields({ kind: 'command' }, 'allow', {})).toEqual({ decision: 'allow' });
    expect(runtimeReplyFields({ kind: 'file' }, 'deny', { q: ['x'] })).toEqual({ decision: 'deny' });
    expect(runtimeReplyFields({ kind: 'command' }, 'allow_always', {})).toEqual({ decision: 'allow_always' });
    expect(runtimeReplyFields({ kind: 'question' }, 'allow', { q: ['x'] })).toEqual({ answers: { q: ['x'] } });
  });
  it('only offers always-allow when the pending request includes that option', () => {
    expect(requestAllowsAlways({})).toBe(false);
    expect(requestAllowsAlways({ permissionOptions: [{ id: 'once', kind: 'allow_once' }] })).toBe(false);
    expect(requestAllowsAlways({
      permissionOptions: [
        { id: 'once', kind: 'allow_once' },
        { id: 'always', kind: 'allow_always' },
      ],
    })).toBe(true);
    expect(requestAllowsAlways({
      permissionOptions: [
        { id: 'once', kind: 'allow_once' },
        { id: 'tool', kind: 'allow_always_tool' },
      ],
    })).toBe(true);
    expect(requestAllowsAlways({
      permissionOptions: [{ id: 'args', kind: 'allow_always_tool_args' }],
    })).toBe(true);
    expect(requestAllowsAlways({
      permissionOptions: [{ id: 'edits', kind: 'allow_edits_for_session' }],
    })).toBe(false);
  });
  it('names always-allow as this conversation for Codex, Grok, and Kiro', () => {
    const request = {
      permissionOptions: [{ id: 'always', kind: 'allow_always' }],
    };
    expect(runtimeAllowAlwaysCopy({ request, agentId: 'grok' })).toEqual({
      shown: true,
      hintKey: 'chat.runtime.allowAlwaysHint',
    });
    expect(runtimeAllowAlwaysCopy({ request, agentId: 'kiro' }).hintKey).toBe(
      'chat.runtime.allowAlwaysHint',
    );
    expect(runtimeAllowAlwaysCopy({ request, agentId: 'codex' })).toEqual({
      shown: true,
      hintKey: 'chat.runtime.allowAlwaysHint',
    });
    expect(runtimeAllowAlwaysCopy({ request: {}, agentId: 'codex' }).shown).toBe(false);
    expect(runtimeAllowAlwaysHintKey('codex')).toBe('chat.runtime.allowAlwaysHint');
    expect(runtimeAllowAlwaysHintKey('claude')).toBe('chat.runtime.allowAlwaysHint');
    const t: TranslateFn = (key, params) => translate('zh', key, params);
    expect(t('chat.runtime.allowAlwaysHint')).toBe('仅当前这次对话，不保存');
    expect(t('chat.runtime.allowAlwaysHintTurn')).toBe('仅当前这次对话，不保存');
    expect(translate('en', 'chat.runtime.allowAlwaysHint')).toBe('This conversation only, not saved');
    expect(translate('en', 'chat.runtime.allowAlwaysHintTurn')).toBe(
      'This conversation only, not saved',
    );
  });
  it('keeps file cards on create/modify/delete and maps English ACP kinds', () => {
    const t: TranslateFn = (key, params) => translate('zh', key, params);
    expect(runtimeRequestTitle(t, { kind: 'file', title: 'Read' })).toBe('修改文件');
    expect(runtimeRequestTitle(t, { kind: 'file', title: '/tmp/a.ts' })).toBe('修改文件');
    expect(runtimeRequestTitle(t, {
      kind: 'file',
      title: '修改文件',
      fileChanges: [{ path: '/tmp/a.ts', kind: 'add' }],
    })).toBe('新增文件');
    expect(runtimeRequestTitle(t, {
      kind: 'file',
      title: '修改文件',
      fileChanges: [{ path: '/tmp/a.ts', kind: 'delete' }],
    })).toBe('删除文件');
    expect(runtimeRequestTitle(t, { kind: 'command', title: 'execute' })).toBe('执行命令');
    expect(runtimeRequestTitle(t, { kind: 'command', title: 'Read' })).toBe('读取文件');
    expect(runtimeRequestTitle(t, { kind: 'command', title: '写文件' })).toBe('写文件');
    expect(runtimeRequestTitle(t, { kind: 'command', title: '' })).toBe('需要确认');
    expect(runtimeRequestTitle(t, { kind: 'question', title: '' })).toBe('需要你的回答');
    expect(runtimeRequestTitle(t, { kind: 'question', title: '选一个模型' })).toBe('选一个模型');
  });
  it('builds a file preview from protocol-copied fixture rows and stays empty for path-only', () => {
    expect(runtimeFileChangePreview({
      kind: 'file',
      detail: '/workspace/qa-codex-filechange-scratch/probe.txt',
      fileChanges: [{
        path: '/workspace/qa-codex-filechange-scratch/probe.txt',
        kind: 'add',
        preview: 'FILECHANGE_OK\n',
      }],
    })).toEqual({
      shown: true,
      empty: false,
      rows: [{
        path: '/workspace/qa-codex-filechange-scratch/probe.txt',
        kind: 'add',
        preview: 'FILECHANGE_OK\n',
      }],
    });
    expect(runtimeFileChangePreview({
      kind: 'file',
      detail: '/workspace/notes.md',
      fileChanges: [{ path: '/workspace/notes.md', kind: 'update' }],
    })).toEqual({
      shown: true,
      empty: true,
      rows: [{ path: '/workspace/notes.md', kind: 'update', preview: null }],
    });
    expect(runtimeFileChangePreview({
      kind: 'file',
      detail: '',
      fileChanges: [],
    })).toEqual({ shown: true, empty: true, rows: [] });
    expect(runtimeFileChangePreview({
      kind: 'command',
      detail: 'ls',
      fileChanges: [],
    })).toEqual({ shown: false });
    expect(fileChangePreviewHintKey({
      shown: true,
      empty: true,
      rows: [{ path: '/workspace/notes.md', kind: 'update', preview: null }],
    })).toBe('chat.runtime.fileChangePathOnly');
    expect(fileChangePreviewHintKey({ shown: true, empty: true, rows: [] }))
      .toBe('chat.runtime.fileChangePreviewEmpty');
  });
  it('keeps a live ACP plan out of empty rows and maps status tone', () => {
    expect(visibleRuntimePlan(undefined)).toEqual([]);
    expect(visibleRuntimePlan([{ content: '  ' }, { content: 'read', status: 'completed' }])).toEqual([
      { content: 'read', status: 'completed' },
    ]);
    expect(runtimePlanEntryTone('in_progress')).toBe('live');
    expect(runtimePlanEntryTone('completed')).toBe('done');
    expect(runtimePlanEntryTone('pending')).toBe('pending');
  });
});
