import type { RuntimeFileChange, RuntimeRequest, RuntimeSnapshot } from '@/lib/api/chat';
import type { MessageKey, TranslateFn } from '@/lib/i18n';

export type RuntimeTransport =
  | { kind: 'runtime'; snapshot: RuntimeSnapshot }
  | { kind: 'legacy'; snapshot: RuntimeSnapshot }
  | { kind: 'unavailable' };

/** A failed runtime read is never permission to start the legacy CLI path. */
export async function readRuntimeTransport(
  read: () => Promise<RuntimeSnapshot>,
): Promise<RuntimeTransport> {
  try {
    const snapshot = await read();
    return snapshot.enabled ? { kind: 'runtime', snapshot } : { kind: 'legacy', snapshot };
  } catch {
    return { kind: 'unavailable' };
  }
}

export function isRuntimeActive(phase: RuntimeSnapshot['phase']): boolean {
  return ['starting', 'running', 'waiting', 'cancelling'].includes(phase);
}

/**
 * Continuous chat composer/send path is Codex, Grok, Kiro, and new-empty Claude
 * (stream-json). Half-surface agents (`cursor`, Claude print+resume history, …)
 * stay off this list — do not invent ChatRuntime just because a CLI has pickers.
 */
export function isRuntimeChatAgent(agentId: string | null | undefined): boolean {
  return (
    agentId === 'codex' ||
    agentId === 'grok' ||
    agentId === 'kiro' ||
    agentId === 'claude'
  );
}

/**
 * Composer chrome follows the conversation's current Agent.
 * A leftover enabled snapshot from Codex / Grok must not keep Pi on that path.
 */
export function bindRuntimeSnapshotToAgent(
  runtime: RuntimeSnapshot | null | undefined,
  extras: { agentId?: string | null; conversationId?: string | null },
): RuntimeSnapshot | null {
  if (!runtime) return null;
  if (!extras.conversationId || runtime.conversationId !== extras.conversationId) return null;
  if (isRuntimeChatAgent(extras.agentId)) return runtime;
  if (!runtime.enabled) return runtime;
  return { ...runtime, enabled: false };
}

export type RuntimeSessionLockExtras = {
  conversationId?: string | null;
  nativeSessionId?: string | null;
  hasMessages?: boolean;
};

/**
 * Empty chats must keep Agent / cwd editable — including Codex rows that
 * advertise `enabled` so the first send uses the runtime path, and a leftover
 * snapshot from the previous conversation. Lock only after *this* conversation
 * has a message, a native thread, or a started runtime session.
 */
export function isRuntimeSessionLocked(
  runtime: Pick<RuntimeSnapshot, 'enabled' | 'phase' | 'runId' | 'conversationId'> | null | undefined,
  extras?: RuntimeSessionLockExtras,
): boolean {
  if (extras?.hasMessages) return true;
  if (extras?.nativeSessionId?.trim()) return true;
  if (!runtime?.enabled) return false;
  if (extras?.conversationId && runtime.conversationId !== extras.conversationId) {
    return false;
  }
  return runtime.phase !== 'idle' || Boolean(runtime.runId);
}

/** A late poll for A must not alter the second visit to A after A → B → A. */
export function acceptsRuntimeSnapshot(
  activeId: string | null,
  activeGeneration: number,
  conversationId: string,
  snapshotGeneration: number,
): boolean {
  return activeId === conversationId && activeGeneration === snapshotGeneration;
}

export function requestMatchesRuntime(request: RuntimeRequest, activeRunId: string | null): boolean {
  return request.runId === activeRunId;
}

export function isLatestRuntimeRead(readId: number, newestReadId: number): boolean {
  return readId === newestReadId;
}

export function canSubmitRuntimeQuestions(
  request: Pick<RuntimeRequest, 'kind' | 'questions'>,
  answers: Record<string, string[]>,
): boolean {
  return request.kind !== 'question' || request.questions.every((question) => Boolean(answers[question.id]?.length));
}

/** ACP remember kinds: standard `allow_always` and Kiro `allow_always_tool` / `_args`. */
export function isRuntimeAllowAlwaysKind(kind: string | null | undefined): boolean {
  return kind === 'allow_always' || Boolean(kind?.startsWith('allow_always_'));
}

/** Session remember is offered only when this request can honor it (ACP option or Codex-synthesized allow_always). */
export function requestAllowsAlways(
  request: Pick<RuntimeRequest, 'permissionOptions'>,
): boolean {
  return (request.permissionOptions ?? []).some((option) => isRuntimeAllowAlwaysKind(option.kind));
}

/**
 * Always-allow is conversation-local and not saved. Codex, Grok, and Kiro
 * all remember later cards in this conversation.
 */
export function runtimeAllowAlwaysHintKey(_agentId?: string | null): MessageKey {
  return 'chat.runtime.allowAlwaysHint';
}

export function runtimeAllowAlwaysCopy(input: {
  request: Pick<RuntimeRequest, 'permissionOptions'>;
  agentId?: string | null;
}): { shown: boolean; hintKey: MessageKey } {
  if (!requestAllowsAlways(input.request)) {
    return { shown: false, hintKey: runtimeAllowAlwaysHintKey(input.agentId) };
  }
  return { shown: true, hintKey: runtimeAllowAlwaysHintKey(input.agentId) };
}

export function runtimeReplyFields(
  request: Pick<RuntimeRequest, 'kind'>,
  decision?: 'allow' | 'deny' | 'allow_always',
  answers?: Record<string, string[]>,
): { decision?: 'allow' | 'deny' | 'allow_always'; answers?: Record<string, string[]> } {
  if (request.kind === 'question') {
    return answers ? { answers } : {};
  }
  return decision ? { decision } : {};
}

const COMMAND_TITLE_KEYS: Record<string, MessageKey> = {
  read: 'chat.runtime.kind.read',
  edit: 'chat.runtime.kind.edit',
  write: 'chat.runtime.kind.write',
  execute: 'chat.runtime.kind.execute',
  exec: 'chat.runtime.kind.execute',
  fetch: 'chat.runtime.kind.fetch',
  search: 'chat.runtime.kind.search',
  delete: 'chat.runtime.kind.delete',
  move: 'chat.runtime.kind.move',
};

/** File cards use create/modify/delete when the protocol named a single kind. */
export function runtimeRequestTitle(
  t: TranslateFn,
  request: Pick<RuntimeRequest, 'kind' | 'title' | 'fileChanges'>,
): string {
  if (request.kind === 'file') return fileChangeTitle(request, t);
  const raw = request.title.trim();
  if (request.kind === 'question') return raw || t('chat.runtime.needAnswer');
  if (!raw) return t('chat.runtime.needConfirm');
  const mapped = COMMAND_TITLE_KEYS[raw.toLowerCase()];
  return mapped ? t(mapped) : raw;
}

function fileChangeTitle(
  request: Pick<RuntimeRequest, 'fileChanges'>,
  t: TranslateFn,
): string {
  const kinds = new Set(
    (request.fileChanges ?? [])
      .map((change) => normalizeFileChangeKind(change.kind))
      .filter((kind): kind is FileChangePreviewKind => Boolean(kind)),
  );
  if (kinds.size === 1 && kinds.has('add')) return t('chat.runtime.fileChangeCreate');
  if (kinds.size === 1 && kinds.has('delete')) return t('chat.runtime.fileChangeDelete');
  return t('chat.runtime.fileChange');
}

export type FileChangePreviewKind = 'add' | 'update' | 'delete';

export type FileChangePreviewRow = {
  path: string;
  kind?: FileChangePreviewKind;
  preview: string | null;
};

export type FileChangePreviewModel =
  | { shown: false }
  | { shown: true; empty: true; rows: FileChangePreviewRow[] }
  | { shown: true; empty: false; rows: FileChangePreviewRow[] };

function normalizeFileChangeKind(kind: string | undefined): FileChangePreviewKind | undefined {
  const raw = kind?.trim().toLowerCase();
  if (raw === 'add' || raw === 'create' || raw === 'create_file' || raw === 'add_file') return 'add';
  if (
    raw === 'update'
    || raw === 'modify'
    || raw === 'edit'
    || raw === 'write'
    || raw === 'update_file'
    || raw === 'modify_file'
    || raw === 'write_file'
  ) {
    return 'update';
  }
  if (raw === 'delete' || raw === 'remove' || raw === 'delete_file' || raw === 'remove_file') return 'delete';
  return undefined;
}

function previewText(change: Pick<RuntimeFileChange, 'preview'>): string | null {
  const text = change.preview;
  if (!text || !text.trim()) return null;
  return text;
}

/** Card preview from protocol-copied rows. Path-only rows stay honest — no fake diff. */
export function runtimeFileChangePreview(
  request: Pick<RuntimeRequest, 'kind' | 'detail' | 'fileChanges'>,
): FileChangePreviewModel {
  const rows: FileChangePreviewRow[] = (request.fileChanges ?? []).map((change) => ({
    path: change.path.trim(),
    kind: normalizeFileChangeKind(change.kind),
    preview: previewText(change),
  })).filter((row) => row.path || row.preview);
  if (rows.length === 0 && request.kind === 'file') {
    const paths = request.detail.split('\n').map((line) => line.trim()).filter(Boolean);
    if (paths.length > 0) {
      rows.push(...paths.map((path) => ({ path, preview: null })));
    }
  }
  if (rows.length === 0) {
    return request.kind === 'file' ? { shown: true, empty: true, rows: [] } : { shown: false };
  }
  const empty = rows.every((row) => !row.preview);
  if (request.kind !== 'file' && empty) return { shown: false };
  return empty ? { shown: true, empty: true, rows } : { shown: true, empty: false, rows };
}

export function fileChangePreviewHintKey(
  preview: FileChangePreviewModel,
): 'chat.runtime.fileChangePathOnly' | 'chat.runtime.fileChangePreviewEmpty' | null {
  if (!preview.shown || !preview.empty) return null;
  return preview.rows.some((row) => row.path)
    ? 'chat.runtime.fileChangePathOnly'
    : 'chat.runtime.fileChangePreviewEmpty';
}

export function fileChangeKindLabel(kind: FileChangePreviewKind, t: TranslateFn): string {
  if (kind === 'add') return t('chat.runtime.fileChangeKindAdd');
  if (kind === 'delete') return t('chat.runtime.fileChangeKindDelete');
  return t('chat.runtime.fileChangeKindUpdate');
}
