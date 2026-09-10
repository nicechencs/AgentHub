/**
 * Chat 过程面板状态机（Phase 0–1）。
 * 从 ChatEvent 推导 per-(turn, agent) 过程视图（命令 / stderr / 步骤）。
 * 设计见 docs/chat-process-streaming.md。
 */

import type { MessageKey, TranslateFn } from '@/lib/i18n';
import type { AgentKey, ChatEvent, ChatMessageStatus, ProcessStep } from '@/lib/types';

export type ProcessPhase =
  | 'queued'
  | 'starting'
  | 'running'
  | 'ok'
  | 'failed'
  | 'cancelled'
  | 'timeout';

export type AgentProcessView = {
  turn: number;
  agent: AgentKey;
  phase: ProcessPhase;
  command?: string;
  stdout: string;
  stderr: string;
  /** Structured steps (tool / thinking / status / raw / usage). Cap in reducer. */
  steps: ProcessStep[];
  updatedAt: number;
};

export type ProcessMap = Record<string, AgentProcessView>;

const MAX_STEPS = 200;

export function processKey(turn: number, agent: AgentKey): string {
  return `${turn}:${agent}`;
}

export function phaseFromMessageStatus(status: ChatMessageStatus | string): ProcessPhase {
  switch (status) {
    case 'running':
      return 'running';
    case 'ok':
    case 'done':
    case 'success':
      return 'ok';
    case 'cancelled':
      return 'cancelled';
    case 'timeout':
      return 'timeout';
    case 'error':
    case 'failed':
      return 'failed';
    default:
      return 'failed';
  }
}

export function processPhaseLabel(phase: ProcessPhase, t: TranslateFn): string {
  switch (phase) {
    case 'queued':
      return t('chat.process.queued');
    case 'starting':
      return t('chat.process.starting');
    case 'running':
      return t('chat.process.running');
    case 'ok':
      return t('chat.process.ok');
    case 'failed':
      return t('chat.process.failed');
    case 'cancelled':
      return t('chat.process.cancelled');
    case 'timeout':
      return t('chat.process.timeout');
    default:
      return phase;
  }
}

/** Map parser raw-step notes to a localized label; keep unknown notes as-is. */
function mapRawStepNote(note: string | null | undefined, t: TranslateFn): string {
  if (!note) return t('chat.process.rawEvent');
  switch (note) {
    case 'unrecognized structured line':
    case '无法识别的输出行':
      return t('chat.process.unrecognizedLine');
    case 'non-json line in structured mode':
    case '结构化模式下出现非 JSON 行':
      return t('chat.process.nonJsonLine');
    case 'line too long':
    case '输出行过长':
      return t('chat.process.lineTooLong');
    default:
      return note;
  }
}

export type ToolActionKind = 'read' | 'edit' | 'execute';
export type ToolActionTone = 'live' | 'done' | 'failed';

type ToolStep = Extract<ProcessStep, { type: 'tool' }>;

const TOOL_LABEL_KEYS = {
  read: {
    live: 'chat.process.toolRead',
    done: 'chat.process.toolReadDone',
    failed: 'chat.process.toolReadFailed',
  },
  edit: {
    live: 'chat.process.toolEdit',
    done: 'chat.process.toolEditDone',
    failed: 'chat.process.toolEditFailed',
  },
  execute: {
    live: 'chat.process.toolRun',
    done: 'chat.process.toolRunDone',
    failed: 'chat.process.toolRunFailed',
  },
} as const satisfies Record<ToolActionKind, Record<ToolActionTone, MessageKey>>;

const TARGET_KEYS = [
  'path',
  'filePath',
  'file_path',
  'target_file',
  'targetFile',
  'file',
  'target',
  'command',
  'cmd',
  'query',
  'pattern',
  'glob',
  'url',
] as const;

function compactToolName(name: string): string {
  return name.trim().toLowerCase().replace(/[\s._-]+/g, '');
}

/** Map vendor tool names (Read / apply_patch / command_execution / …) to a user verb. */
export function classifyToolAction(name: string): ToolActionKind {
  const compact = compactToolName(name);
  if (!compact) return 'execute';
  if (isReadToolName(compact)) return 'read';
  if (isEditToolName(compact)) return 'edit';
  return 'execute';
}

function isReadToolName(compact: string): boolean {
  if (
    compact === 'read' ||
    compact === 'view' ||
    compact === 'cat' ||
    compact === 'glob' ||
    compact === 'grep' ||
    compact === 'search' ||
    compact === 'ls' ||
    compact === 'find' ||
    compact === 'fetch' ||
    compact === 'get' ||
    compact === 'inspect'
  ) {
    return true;
  }
  return (
    compact.startsWith('read') ||
    compact.startsWith('glob') ||
    compact.startsWith('grep') ||
    compact.includes('search') ||
    compact.includes('webfetch') ||
    compact.includes('listdir') ||
    compact.includes('listfile') ||
    compact.includes('filesearch')
  );
}

function isEditToolName(compact: string): boolean {
  if (
    compact === 'write' ||
    compact === 'edit' ||
    compact === 'delete' ||
    compact === 'move' ||
    compact === 'patch' ||
    compact === 'replace' ||
    compact === 'rename'
  ) {
    return true;
  }
  return (
    compact.startsWith('write') ||
    compact.startsWith('edit') ||
    compact.includes('strreplace') ||
    compact.includes('applypatch') ||
    compact.includes('filechange') ||
    compact.includes('notebook')
  );
}

export function toolActionTone(status: string | undefined | null): ToolActionTone {
  const s = (status ?? '').trim().toLowerCase();
  if (
    s === 'error' ||
    s === 'failed' ||
    s === 'fail' ||
    s === 'cancelled' ||
    s === 'canceled' ||
    s === 'timeout'
  ) {
    return 'failed';
  }
  if (
    s === 'end' ||
    s === 'completed' ||
    s === 'complete' ||
    s === 'success' ||
    s === 'ok' ||
    s === 'done'
  ) {
    return 'done';
  }
  return 'live';
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function firstStringList(value: unknown): string | undefined {
  if (typeof value === 'string' && value.trim()) return value.trim();
  if (!Array.isArray(value)) return undefined;
  const parts = value.filter((item): item is string => typeof item === 'string' && Boolean(item.trim()));
  return parts.length > 0 ? parts.join(' ') : undefined;
}

/** Keep a path or command short enough for a process row. */
export function shortenToolTarget(value: string): string {
  const oneLine = value.replace(/\s+/g, ' ').trim();
  if (!oneLine) return '';
  if ((oneLine.includes('/') || oneLine.includes('\\')) && !oneLine.includes(' ')) {
    const parts = oneLine.replace(/\\/g, '/').split('/').filter(Boolean);
    if (parts.length > 2) return parts.slice(-2).join('/');
  }
  if (oneLine.length > 56) return `${oneLine.slice(0, 28)}…${oneLine.slice(-24)}`;
  return oneLine;
}

function targetFromInput(input: unknown): string | undefined {
  const direct = firstStringList(input);
  if (direct) return shortenToolTarget(direct);
  const rec = asRecord(input);
  if (!rec) return undefined;
  for (const key of TARGET_KEYS) {
    const found = firstStringList(rec[key]);
    if (found) return shortenToolTarget(found);
  }
  return undefined;
}

function targetFromToolName(name: string): string | undefined {
  const trimmed = name.trim();
  const parts = trimmed.split(/\s+/).filter(Boolean);
  if (parts.length < 2) return undefined;
  const last = parts[parts.length - 1];
  if (!last || last === name) return undefined;
  if (!/[/\\.]/.test(last)) return undefined;
  return shortenToolTarget(last);
}

export function toolActionTarget(name: string, input?: unknown): string | undefined {
  return targetFromInput(input) ?? targetFromToolName(name);
}

export function formatToolStep(step: ToolStep, t: TranslateFn): string {
  const kind = classifyToolAction(step.name);
  const tone = toolActionTone(step.status);
  const label = t(TOOL_LABEL_KEYS[kind][tone]);
  const target = toolActionTarget(step.name, step.input);
  return target ? `${label} ${target}` : label;
}

/** Protocol-only rows (item types, retry, plan) stay in the folded details. */
export function isProtocolProcessStep(step: ProcessStep): boolean {
  return step.type === 'status';
}

function lastMatching<T>(items: T[], pred: (item: T) => boolean): T | undefined {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    if (pred(items[i])) return items[i];
  }
  return undefined;
}

/** Collapsed-panel headline: 正在读取 / 正在修改 / 正在执行, not tool names. */
export function formatProcessHeadline(
  steps: ProcessStep[],
  phase: ProcessPhase,
  t: TranslateFn,
): string {
  const tools = steps.filter((step): step is ToolStep => step.type === 'tool');
  const lastLive = lastMatching(tools, (step) => toolActionTone(step.status) === 'live');
  if (lastLive) return formatToolStep(lastLive, t);

  const lastThinking = lastMatching(steps, (step) => step.type === 'thinking');
  if (lastThinking?.type === 'thinking' && !lastThinking.done) {
    return t('chat.process.thinking');
  }

  if (phase === 'queued' || phase === 'starting' || phase === 'running') {
    if (tools.length > 0) return formatToolStep(tools[tools.length - 1], t);
    return t('chat.process.summaryGenerating');
  }

  if (phase === 'failed' || phase === 'timeout') {
    const lastFailed = lastMatching(tools, (step) => toolActionTone(step.status) === 'failed');
    if (lastFailed) return formatToolStep(lastFailed, t);
  }

  const kinds = new Set(tools.map((step) => classifyToolAction(step.name)));
  const done: string[] = [];
  if (kinds.has('read')) done.push(t('chat.process.toolReadDone'));
  if (kinds.has('edit')) done.push(t('chat.process.toolEditDone'));
  if (kinds.has('execute')) done.push(t('chat.process.toolRunDone'));
  const phaseLabel = processPhaseLabel(phase, t);
  return done.length > 0 ? `${phaseLabel} · ${done.join(' · ')}` : phaseLabel;
}

export function stepSummary(step: ProcessStep, t: TranslateFn): string {
  switch (step.type) {
    case 'status':
      return step.detail ? `${step.phase} · ${step.detail}` : step.phase;
    case 'thinking':
      return step.done ? t('chat.process.thinkingDone') : t('chat.process.thinking');
    case 'tool':
      return formatToolStep(step, t);
    case 'text':
      return t('chat.process.text');
    case 'raw':
      return mapRawStepNote(step.note, t);
    case 'error':
      return step.message;
    case 'usage':
      return formatUsageStep(step, t);
    default:
      return 'step';
  }
}

export type UsageStep = Extract<ProcessStep, { type: 'usage' }>;

export function usageScope(step: UsageStep): 'turn' | 'session' {
  return step.scope === 'session' ? 'session' : 'turn';
}

export function usageByScope(steps: ProcessStep[] | undefined): {
  turn?: UsageStep;
  session?: UsageStep;
} {
  const out: { turn?: UsageStep; session?: UsageStep } = {};
  if (!steps) return out;
  for (const step of steps) {
    if (step.type !== 'usage') continue;
    out[usageScope(step)] = step;
  }
  return out;
}

function formatUsageCounts(step: UsageStep, t: TranslateFn): string {
  const parts: string[] = [];
  if (step.input != null) parts.push(t('chat.process.usageInput', { n: step.input }));
  if (step.output != null) parts.push(t('chat.process.usageOutput', { n: step.output }));
  if (step.cacheRead) parts.push(t('chat.process.usageCache', { n: step.cacheRead }));
  if (step.cacheWrite) parts.push(t('chat.process.usageCacheWrite', { n: step.cacheWrite }));
  return parts.join(' · ');
}

/** One usage row: 当前轮 or 累计, protocol fields as sent. Cache only when > 0. */
export function formatUsageStep(step: UsageStep, t: TranslateFn): string {
  const counts = formatUsageCounts(step, t);
  const label =
    usageScope(step) === 'session' ? t('chat.process.usageSession') : t('chat.process.usageTurn');
  const window =
    usageScope(step) === 'session' && step.total != null && step.contextWindow
      ? t('chat.process.usageWindow', { used: step.total, window: step.contextWindow })
      : '';
  const body = [counts, window].filter(Boolean).join(' · ');
  return body ? `${label} ${body}` : label;
}

/** Reply-header line: 用量 + 当前轮 and 累计 when the Agent sent them. */
export function formatVisibleUsage(steps: ProcessStep[] | undefined, t: TranslateFn): string {
  const { turn, session } = usageByScope(steps);
  const parts: string[] = [];
  if (turn) parts.push(formatUsageStep(turn, t));
  if (session) parts.push(formatUsageStep(session, t));
  if (parts.length === 0) return '';
  return `${t('chat.process.usage')} ${parts.join(' · ')}`;
}

/**
 * After the turn ends: muted footnote under the reply.
 * Turn-scope counts only — never session total or context window.
 */
export function formatTurnUsageFooter(
  steps: ProcessStep[] | undefined,
  running: boolean,
  t: TranslateFn,
): string {
  if (running) return '';
  const { turn } = usageByScope(steps);
  if (!turn) return '';
  return formatUsageCounts(turn, t);
}

/** Timeline worth opening in the inspect pane (not usage-only, not protocol-only). */
export function hasInspectableProcess(view: AgentProcessView | undefined): boolean {
  if (!view) return false;
  if (view.command || view.stderr) return true;
  return view.steps.some(
    (step) => step.type !== 'usage' && step.type !== 'text' && !isProtocolProcessStep(step),
  );
}

/** 是否值得展示过程折叠面板 */
export function hasProcessDetails(view: AgentProcessView | undefined): boolean {
  if (!view) return false;
  return Boolean(
    view.command ||
      view.stderr ||
      view.steps.length > 0 ||
      view.phase === 'queued' ||
      view.phase === 'starting' ||
      view.phase === 'running' ||
      view.phase === 'failed' ||
      view.phase === 'cancelled' ||
      view.phase === 'timeout',
  );
}

function emptyView(turn: number, agent: AgentKey, phase: ProcessPhase, now: number): AgentProcessView {
  return {
    turn,
    agent,
    phase,
    stdout: '',
    stderr: '',
    steps: [],
    updatedAt: now,
  };
}

function markLastThinkingDone(steps: ProcessStep[]): ProcessStep[] {
  for (let i = steps.length - 1; i >= 0; i -= 1) {
    const row = steps[i];
    if (row.type === 'thinking') {
      if (row.done) return steps;
      const next = steps.slice();
      next[i] = { ...row, done: true };
      return next;
    }
    if (row.type !== 'status') break;
  }
  return steps;
}

/**
 * Codex `item.updated` reasoning is a full snapshot; Grok/Pi/Claude thinking
 * chunks are deltas. If the new text already contains the previous text as a
 * prefix, replace; a later shorter prefix is a replay and is ignored.
 */
export function mergeThinkingText(prev: string, next: string): string {
  if (!next) return prev;
  if (!prev) return next;
  if (next.startsWith(prev)) return next;
  if (prev.startsWith(next)) return prev;
  return `${prev}${next}`;
}

function mergeToolStep(prev: Extract<ProcessStep, { type: 'tool' }>, step: Extract<ProcessStep, { type: 'tool' }>): ProcessStep {
  const name =
    step.name && step.name !== 'tool' ? step.name : prev.name || step.name;
  return {
    type: 'tool',
    id: step.id ?? prev.id,
    name,
    input: step.input !== undefined ? step.input : prev.input,
    status: step.status || prev.status,
    result: step.result != null && step.result !== '' ? step.result : prev.result,
  };
}

function isPriorityStep(step: ProcessStep): boolean {
  return step.type === 'tool' || step.type === 'error' || step.type === 'usage';
}

/**
 * 过程步封顶：优先保留 tool / error / usage（对齐 core MAX_EMITTED_STEPS 对 Error/Tool/Usage 的突破）。
 * 其余类型从最旧开始丢；若优先步本身超过上限，只留最近 MAX_STEPS 条并丢掉全部 soft 步。
 */
function capSteps(steps: ProcessStep[]): ProcessStep[] {
  if (steps.length <= MAX_STEPS) return steps;

  const priority = steps.filter(isPriorityStep);
  if (priority.length >= MAX_STEPS) {
    return priority.slice(priority.length - MAX_STEPS);
  }

  const dropSoft = steps.length - MAX_STEPS;
  let dropped = 0;
  return steps.filter((step) => {
    if (dropped >= dropSoft || isPriorityStep(step)) return true;
    dropped += 1;
    return false;
  });
}

function pushStep(steps: ProcessStep[], step: ProcessStep): ProcessStep[] {
  if (step.type === 'text' && steps.length > 0) {
    const last = steps[steps.length - 1];
    if (last.type === 'text') {
      const next = steps.slice(0, -1);
      next.push({ type: 'text', text: last.text + step.text });
      return capSteps(next);
    }
  }

  if (step.type === 'thinking') {
    const last = steps[steps.length - 1];
    if (last?.type === 'thinking' && !last.done) {
      const next = steps.slice(0, -1);
      next.push({
        type: 'thinking',
        text: mergeThinkingText(last.text, step.text),
        done: Boolean(step.done),
      });
      return capSteps(next);
    }
  }

  if (step.type === 'tool' && step.id) {
    const idx = findLastIndex(steps, (row) => row.type === 'tool' && row.id === step.id);
    if (idx >= 0) {
      const prev = steps[idx];
      if (prev.type === 'tool') {
        const next = steps.slice();
        next[idx] = mergeToolStep(prev, step);
        return next;
      }
    }
  }

  if (step.type === 'usage') {
    const scope = usageScope(step);
    const idx = findLastIndex(
      steps,
      (row) => row.type === 'usage' && usageScope(row) === scope,
    );
    if (idx >= 0) {
      const next = steps.slice();
      next[idx] = step;
      return next;
    }
  }

  const base = step.type === 'thinking' ? steps : markLastThinkingDone(steps);
  return capSteps([...base, step]);
}

function findLastIndex<T>(items: T[], pred: (item: T) => boolean): number {
  for (let i = items.length - 1; i >= 0; i -= 1) {
    if (pred(items[i])) return i;
  }
  return -1;
}

/**
 * 纯函数：应用一条 ChatEvent，返回新的 ProcessMap。
 * 未知 / 无关事件原样返回同一引用（便于 React bail-out）。
 */
export function reduceProcessEvent(map: ProcessMap, ev: ChatEvent, now = Date.now()): ProcessMap {
  if (ev.type === 'started') {
    const next: ProcessMap = { ...map };
    for (const agent of ev.agents) {
      const key = processKey(ev.turn, agent);
      next[key] = emptyView(ev.turn, agent, 'queued', now);
    }
    return next;
  }

  if (ev.type === 'agentStarted') {
    const key = processKey(ev.turn, ev.agent);
    const prev = map[key];
    return {
      ...map,
      [key]: {
        turn: ev.turn,
        agent: ev.agent,
        phase: 'running',
        command: ev.command,
        stdout: prev?.stdout ?? '',
        stderr: prev?.stderr ?? '',
        steps: prev?.steps ?? [],
        updatedAt: now,
      },
    };
  }

  if (ev.type === 'agentChunk') {
    const key = processKey(ev.turn, ev.agent);
    const prev = map[key] ?? emptyView(ev.turn, ev.agent, 'running', now);
    const stdout = ev.stream === 'stdout' ? prev.stdout + ev.text : prev.stdout;
    const stderr = ev.stream === 'stderr' ? prev.stderr + ev.text : prev.stderr;
    return {
      ...map,
      [key]: {
        ...prev,
        phase: prev.phase === 'queued' || prev.phase === 'starting' ? 'running' : prev.phase,
        stdout,
        stderr,
        updatedAt: now,
      },
    };
  }

  if (ev.type === 'agentProcess') {
    const key = processKey(ev.turn, ev.agent);
    const prev = map[key] ?? emptyView(ev.turn, ev.agent, 'running', now);
    // Skip pure text steps in the timeline (already in bubble body); keep tool/thinking/status.
    if (ev.step.type === 'text') {
      return {
        ...map,
        [key]: {
          ...prev,
          phase: prev.phase === 'queued' || prev.phase === 'starting' ? 'running' : prev.phase,
          updatedAt: now,
        },
      };
    }
    return {
      ...map,
      [key]: {
        ...prev,
        phase: prev.phase === 'queued' || prev.phase === 'starting' ? 'running' : prev.phase,
        steps: pushStep(prev.steps, ev.step),
        updatedAt: now,
      },
    };
  }

  if (ev.type === 'agentFinished') {
    const key = processKey(ev.turn, ev.agent);
    const prev = map[key] ?? emptyView(ev.turn, ev.agent, 'running', now);
    const content = ev.message.content ?? '';
    return {
      ...map,
      [key]: {
        ...prev,
        phase: phaseFromMessageStatus(ev.message.status),
        stdout: content || prev.stdout,
        steps: markLastThinkingDone(prev.steps),
        updatedAt: now,
      },
    };
  }

  // 回合总结束：把仍停在进行中的过程项收成终态，保证 UI 能自动折叠
  if (ev.type === 'finished') {
    let changed = false;
    const next: ProcessMap = { ...map };
    for (const [key, view] of Object.entries(map)) {
      if (
        view.turn === ev.turn &&
        (view.phase === 'queued' ||
          view.phase === 'starting' ||
          view.phase === 'running')
      ) {
        next[key] = {
          ...view,
          // 生产取消时 ok=true；缺省 cancelled 当 false，兼容旧事件
          phase: ev.cancelled ? 'cancelled' : ev.ok ? 'ok' : 'failed',
          steps: markLastThinkingDone(view.steps),
          updatedAt: now,
        };
        changed = true;
      }
    }
    return changed ? next : map;
  }

  return map;
}
