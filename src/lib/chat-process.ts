/**
 * Chat 过程面板状态机（Phase 0–1）。
 * 从 ChatEvent 推导 per-(turn, agent) 过程视图（命令 / stderr / 步骤）。
 * 设计见 docs/chat-process-streaming.md。
 */

import { interpolate, type MessageKey, type MessageParams, type TranslateFn } from '@/lib/i18n';
import type { AgentId, ChatEvent, ChatMessageStatus, ProcessStep } from '@/lib/types';

function tx(t: TranslateFn | undefined, key: MessageKey, fallback: string, params?: MessageParams): string {
  return t ? t(key, params) : interpolate(fallback, params);
}

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
  agent: AgentId;
  phase: ProcessPhase;
  command?: string;
  stdout: string;
  stderr: string;
  /** Structured steps (tool / thinking / status / raw). Cap in reducer. */
  steps: ProcessStep[];
  updatedAt: number;
};

export type ProcessMap = Record<string, AgentProcessView>;

const MAX_STEPS = 200;

export function processKey(turn: number, agent: AgentId): string {
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

export function processPhaseLabel(phase: ProcessPhase, t?: TranslateFn): string {
  switch (phase) {
    case 'queued':
      return tx(t, 'chat.process.queued', '排队中');
    case 'starting':
      return tx(t, 'chat.process.starting', '启动中');
    case 'running':
      return tx(t, 'chat.process.running', '生成中');
    case 'ok':
      return tx(t, 'chat.process.ok', '已完成');
    case 'failed':
      return tx(t, 'chat.process.failed', '失败');
    case 'cancelled':
      return tx(t, 'chat.process.cancelled', '已取消');
    case 'timeout':
      return tx(t, 'chat.process.timeout', '超时');
    default:
      return phase;
  }
}

export function stepSummary(step: ProcessStep, t?: TranslateFn): string {
  switch (step.type) {
    case 'status':
      return step.detail ? `${step.phase} · ${step.detail}` : step.phase;
    case 'thinking':
      return step.done
        ? tx(t, 'chat.process.thinkingDone', '思考完成')
        : tx(t, 'chat.process.thinking', '思考中');
    case 'tool': {
      const st = step.status || '';
      return st ? `${step.name} (${st})` : step.name;
    }
    case 'text':
      return tx(t, 'chat.process.text', '文本');
    case 'raw':
      return step.note || tx(t, 'chat.process.rawEvent', '原始事件');
    case 'error':
      return step.message;
    default:
      return 'step';
  }
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

function emptyView(turn: number, agent: AgentId, phase: ProcessPhase, now: number): AgentProcessView {
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

function pushStep(steps: ProcessStep[], step: ProcessStep): ProcessStep[] {
  // Collapse consecutive tiny text steps into one for UI noise control.
  if (step.type === 'text' && steps.length > 0) {
    const last = steps[steps.length - 1];
    if (last.type === 'text') {
      const next = steps.slice(0, -1);
      next.push({ type: 'text', text: last.text + step.text });
      return next.length > MAX_STEPS ? next.slice(next.length - MAX_STEPS) : next;
    }
  }
  const next = [...steps, step];
  return next.length > MAX_STEPS ? next.slice(next.length - MAX_STEPS) : next;
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
          phase: ev.ok ? 'ok' : 'failed',
          updatedAt: now,
        };
        changed = true;
      }
    }
    return changed ? next : map;
  }

  return map;
}
