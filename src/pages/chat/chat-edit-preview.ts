import {
  classifyToolAction,
  toolActionTone,
  type ProcessMap,
} from '@/lib/chat-process';
import type { ProcessStep } from '@/lib/types';

export type TurnEditStatus = 'live' | 'done';

export type TurnEditFile = {
  path: string;
  status: TurnEditStatus;
  before?: string;
  after?: string;
  diff?: string;
};

const PATH_KEYS = [
  'path',
  'file',
  'filePath',
  'file_path',
  'target_file',
  'targetFile',
  'uri',
  'fileUri',
  'file_uri',
] as const;

const BEFORE_KEYS = ['before', 'oldText', 'old_text', 'old_string'] as const;
const AFTER_KEYS = ['content', 'contents', 'newText', 'new_text', 'new_string', 'after'] as const;

type ToolStep = Extract<ProcessStep, { type: 'tool' }>;

type CollectedEdit = {
  path: string;
  before?: string;
  after?: string;
  diff?: string;
};

export function latestProcessTurn(processMap: ProcessMap): number | null {
  let max = Number.NEGATIVE_INFINITY;
  for (const view of Object.values(processMap)) {
    if (typeof view.turn === 'number' && view.turn > max) max = view.turn;
  }
  return Number.isFinite(max) ? max : null;
}

export function sameEditPath(a: string, b: string): boolean {
  return normalizeEditPath(a) === normalizeEditPath(b);
}

export function turnEditHasInlineDiff(file: TurnEditFile): boolean {
  return Boolean(turnEditDiffText(file));
}

/** Unified diff already on the tool payload, or a simple diff from old+new text. */
export function turnEditDiffText(file: Pick<TurnEditFile, 'before' | 'after' | 'diff' | 'path'>): string | null {
  const patch = file.diff?.trim() ? file.diff : '';
  if (patch && looksLikeUnifiedDiff(patch)) return file.diff ?? patch;
  if (file.before != null && file.after != null && file.before !== file.after) {
    return formatSimpleDiff(file.before, file.after, file.path);
  }
  return null;
}

export function formatSimpleDiff(before: string, after: string, path = ''): string {
  const a = before.split('\n');
  const b = after.split('\n');
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start += 1;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA -= 1;
    endB -= 1;
  }
  const minusCount = Math.max(0, endA - start);
  const plusCount = Math.max(0, endB - start);
  const lines: string[] = [];
  if (path.trim()) {
    lines.push(`--- ${path.trim()}`);
    lines.push(`+++ ${path.trim()}`);
  }
  lines.push(`@@ -${start + 1},${minusCount} +${start + 1},${plusCount} @@`);
  for (let i = start; i < endA; i += 1) lines.push(`-${a[i]}`);
  for (let i = start; i < endB; i += 1) lines.push(`+${b[i]}`);
  return lines.join('\n');
}

export function extractEditFilesFromSteps(steps: ProcessStep[]): TurnEditFile[] {
  const files: TurnEditFile[] = [];
  for (const step of steps) {
    if (step.type !== 'tool') continue;
    if (classifyToolAction(step.name) !== 'edit') continue;
    const tone = toolActionTone(step.status);
    if (tone === 'failed') continue;
    const status: TurnEditStatus = tone === 'live' ? 'live' : 'done';
    const collected = filesFromToolStep(step);
    for (const item of collected) {
      upsertEditFile(files, { ...item, status });
    }
  }
  return files;
}

/** This turn's 正在修改 / 已修改 files. Defaults to the latest turn in the map. */
export function extractTurnEdits(processMap: ProcessMap, turn?: number): TurnEditFile[] {
  const targetTurn = turn ?? latestProcessTurn(processMap);
  if (targetTurn == null) return [];
  const views = Object.values(processMap)
    .filter((view) => view.turn === targetTurn)
    .sort((a, b) => a.updatedAt - b.updatedAt);
  const files: TurnEditFile[] = [];
  for (const view of views) {
    for (const file of extractEditFilesFromSteps(view.steps)) {
      upsertEditFile(files, file);
    }
  }
  return files;
}

function filesFromToolStep(step: ToolStep): CollectedEdit[] {
  const fromInput = collectEdits(step.input, 0);
  const parsedResult = parseMaybeJson(step.result);
  const fromResult = collectEdits(parsedResult, 0);
  const merged = mergeCollected(fromInput, fromResult);
  if (merged.length === 1) {
    const orphan = textsFromUnknown(parsedResult);
    if (orphan) merged[0] = overlayTexts(merged[0], orphan);
  }
  if (merged.length > 0) return merged;
  const fallback = pathFromUnknown(step.input) ?? pathFromToolName(step.name);
  return fallback ? [{ path: fallback }] : [];
}

function textsFromUnknown(value: unknown): Pick<CollectedEdit, 'before' | 'after' | 'diff'> | null {
  if (typeof value === 'string' && looksLikeUnifiedDiff(value)) return { diff: value };
  const rec = asRecord(value);
  if (!rec) return null;
  const before = firstText(rec, BEFORE_KEYS);
  const after = firstText(rec, AFTER_KEYS);
  const diff = firstText(rec, ['diff', 'patch'] as const);
  if (before == null && after == null && diff == null) return null;
  const out: Pick<CollectedEdit, 'before' | 'after' | 'diff'> = {};
  if (before != null) out.before = before;
  if (after != null) out.after = after;
  if (diff != null) out.diff = diff;
  return out;
}

function overlayTexts(row: CollectedEdit, extra: Pick<CollectedEdit, 'before' | 'after' | 'diff'>): CollectedEdit {
  return {
    path: row.path,
    before: row.before ?? extra.before,
    after: row.after ?? extra.after,
    diff: row.diff ?? extra.diff,
  };
}

function collectEdits(value: unknown, depth: number): CollectedEdit[] {
  if (depth > 3) return [];
  const rec = asRecord(value);
  if (!rec) {
    const path = typeof value === 'string' ? normalizePath(value) : undefined;
    return path && looksLikeFilePath(path) ? [{ path }] : [];
  }

  const fromChanges = collectFromArray(rec.changes, depth);
  if (fromChanges.length) return fromChanges;

  const fileChanges = asRecord(rec.fileChanges);
  if (fileChanges) {
    const rows: CollectedEdit[] = [];
    for (const [path, row] of Object.entries(fileChanges)) {
      rows.push(...pushChange(row, path, depth));
    }
    if (rows.length) return rows;
  }

  const fromLocations = collectFromArray(rec.locations, depth);
  if (fromLocations.length) return fromLocations;

  const fromFiles = collectFilesField(rec.files, depth);
  if (fromFiles.length) return fromFiles;

  if (rec.operation != null) {
    const fromOperation = pushChange(rec.operation, undefined, depth);
    if (fromOperation.length) return fromOperation;
  }

  const self = pushChange(rec, undefined, depth);
  if (self.length) return self;

  if (rec.item != null) {
    const nested = collectEdits(rec.item, depth + 1);
    if (nested.length) return nested;
  }

  const rawInput = rec.toolCall && asRecord(rec.toolCall)?.rawInput;
  if (rawInput != null) {
    const nested = collectEdits(rawInput, depth + 1);
    if (nested.length) return nested;
  }

  if (rec.toolCall != null) {
    const nested = collectEdits(rec.toolCall, depth + 1);
    if (nested.length) return nested;
  }

  return [];
}

function collectFromArray(value: unknown, depth: number): CollectedEdit[] {
  if (!Array.isArray(value)) return [];
  const rows: CollectedEdit[] = [];
  for (const item of value) {
    rows.push(...pushChange(item, typeof item === 'string' ? item : undefined, depth));
  }
  return rows;
}

function collectFilesField(value: unknown, depth: number): CollectedEdit[] {
  if (!Array.isArray(value)) return [];
  const rows: CollectedEdit[] = [];
  for (const item of value) {
    if (typeof item === 'string') {
      const path = normalizePath(item);
      if (path) rows.push({ path });
      continue;
    }
    rows.push(...pushChange(item, undefined, depth));
  }
  return rows;
}

function pushChange(value: unknown, fallbackPath: string | undefined, depth: number): CollectedEdit[] {
  const rec = asRecord(value);
  const path = (rec ? pathFromRecord(rec) : undefined)
    ?? (fallbackPath ? normalizePath(fallbackPath) : undefined);
  if (!path) {
    if (rec && depth < 3) return collectEdits(value, depth + 1);
    return [];
  }
  const before = rec ? firstText(rec, BEFORE_KEYS) : undefined;
  const after = rec ? firstText(rec, AFTER_KEYS) : undefined;
  const diff = rec ? firstText(rec, ['diff', 'patch'] as const) : undefined;
  const row: CollectedEdit = { path };
  if (before != null) row.before = before;
  if (after != null) row.after = after;
  if (diff != null) row.diff = diff;
  return [row];
}

function mergeCollected(first: CollectedEdit[], second: CollectedEdit[]): CollectedEdit[] {
  const out: CollectedEdit[] = [];
  for (const item of first) upsertCollected(out, item);
  for (const item of second) upsertCollected(out, item);
  return out;
}

function upsertCollected(files: CollectedEdit[], next: CollectedEdit): void {
  const index = files.findIndex((row) => sameEditPath(row.path, next.path));
  if (index < 0) {
    files.push(next);
    return;
  }
  const prev = files[index];
  files[index] = {
    path: next.path || prev.path,
    before: next.before ?? prev.before,
    after: next.after ?? prev.after,
    diff: next.diff ?? prev.diff,
  };
}

function upsertEditFile(files: TurnEditFile[], next: TurnEditFile): void {
  const index = files.findIndex((row) => sameEditPath(row.path, next.path));
  if (index < 0) {
    files.push(next);
    return;
  }
  const prev = files[index];
  files[index] = {
    path: next.path || prev.path,
    status: next.status,
    before: next.before ?? prev.before,
    after: next.after ?? prev.after,
    diff: next.diff ?? prev.diff,
  };
}

function pathFromUnknown(value: unknown): string | undefined {
  if (typeof value === 'string') {
    const path = normalizePath(value);
    return path && looksLikeFilePath(path) ? path : undefined;
  }
  const rec = asRecord(value);
  return rec ? pathFromRecord(rec) : undefined;
}

function pathFromRecord(rec: Record<string, unknown>): string | undefined {
  for (const key of PATH_KEYS) {
    const found = firstString(rec[key]);
    const path = found ? normalizePath(found) : undefined;
    if (path) return path;
  }
  return undefined;
}

function pathFromToolName(name: string): string | undefined {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length < 2) return undefined;
  const last = parts[parts.length - 1];
  if (!last || !looksLikeFilePath(last)) return undefined;
  return normalizePath(last);
}

function looksLikeFilePath(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed || /\s/.test(trimmed) && !/[/\\]/.test(trimmed)) return false;
  return /[/\\]/.test(trimmed) || /\.[A-Za-z0-9]{1,8}$/.test(trimmed);
}

function normalizePath(raw: string): string | undefined {
  let trimmed = raw.trim();
  if (!trimmed) return undefined;
  if (trimmed.startsWith('file://')) {
    trimmed = trimmed.slice('file://'.length);
    if (trimmed.toLowerCase().startsWith('localhost')) {
      trimmed = trimmed.slice('localhost'.length);
    }
  }
  return trimmed || undefined;
}

function normalizeEditPath(path: string): string {
  return path.trim().replace(/\\/g, '/').replace(/\/+$/, '');
}

function looksLikeUnifiedDiff(text: string): boolean {
  return /^(diff --git |--- |\+\+\+ |@@ )/m.test(text);
}

function parseMaybeJson(value: unknown): unknown {
  if (typeof value !== 'string') return value;
  const trimmed = value.trim();
  if (!trimmed) return value;
  const start = trimmed[0];
  if (start !== '{' && start !== '[') return value;
  try {
    return JSON.parse(trimmed) as unknown;
  } catch {
    return value;
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function firstText(rec: Record<string, unknown>, keys: readonly string[]): string | undefined {
  for (const key of keys) {
    const found = firstString(rec[key]);
    if (found != null) return found;
  }
  return undefined;
}

function firstString(value: unknown): string | undefined {
  if (typeof value === 'string' && value.trim()) return value;
  if (!Array.isArray(value)) return undefined;
  const parts = value.filter((item): item is string => typeof item === 'string' && Boolean(item.trim()));
  return parts.length > 0 ? parts.join('\n') : undefined;
}
