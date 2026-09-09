import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { AgentProcessView } from '@/lib/chat-process';
import {
  RUNTIME_SNAPSHOT_POLL_ACTIVE_MS,
  RUNTIME_SNAPSHOT_POLL_BACKGROUND_MS,
  streamingActivity,
  streamingPlaceholderKey,
  streamingStatusKey,
} from './chat-streaming';

const t = createTranslator('zh');

function view(steps: AgentProcessView['steps'] = []): AgentProcessView {
  return {
    turn: 1,
    agent: 'codex',
    phase: 'running',
    stdout: '',
    stderr: '',
    steps,
    updatedAt: 0,
  };
}

describe('runtime snapshot poll cadence', () => {
  it('polls the focused turn faster than 400ms without inventing a token drip', () => {
    expect(RUNTIME_SNAPSHOT_POLL_ACTIVE_MS).toBeLessThan(400);
    expect(RUNTIME_SNAPSHOT_POLL_ACTIVE_MS).toBeGreaterThanOrEqual(50);
    expect(RUNTIME_SNAPSHOT_POLL_BACKGROUND_MS).toBe(400);
  });
});

describe('streamingActivity', () => {
  it('shows thinking before the first character', () => {
    expect(streamingActivity(undefined, false)).toBe('thinking');
    expect(streamingActivity(view(), false)).toBe('thinking');
    expect(streamingActivity(view([{ type: 'thinking', text: '规划', done: false }]), false)).toBe(
      'thinking',
    );
  });

  it('shows writing once durable text exists, even if a thinking row is still open', () => {
    expect(
      streamingActivity(view([{ type: 'thinking', text: '规划', done: false }]), true),
    ).toBe('writing');
  });

  it('shows writing after thinking finishes and before the first character', () => {
    expect(streamingActivity(view([{ type: 'thinking', text: '规划', done: true }]), false)).toBe(
      'writing',
    );
  });
});

describe('streaming copy', () => {
  it('uses 正在想 / 正在写 in Chinese', () => {
    expect(t(streamingPlaceholderKey(view()))).toBe('正在想');
    expect(t(streamingPlaceholderKey(view([{ type: 'thinking', text: '', done: true }])))).toBe(
      '正在写',
    );
    expect(t(streamingStatusKey(view(), false))).toBe('正在想');
    expect(t(streamingStatusKey(view(), true))).toBe('正在写');
  });
});
