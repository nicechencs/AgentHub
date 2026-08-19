import { describe, expect, it } from 'vitest';
import {
  formatResumeCommand,
  nativeResumeCommand,
  planNativeResume,
} from './session-resume';

describe('planNativeResume', () => {
  it('matches herdr resume argv for overlapping agents', () => {
    expect(planNativeResume('claude', 'claude-session')?.argv).toEqual([
      'claude',
      '--resume',
      'claude-session',
    ]);
    expect(planNativeResume('codex', 'codex-session')?.argv).toEqual([
      'codex',
      'resume',
      'codex-session',
    ]);
    expect(planNativeResume('kimi', 'kimi-session')?.argv).toEqual([
      'kimi',
      '--session',
      'kimi-session',
    ]);
    expect(planNativeResume('grok', 'grok-session')?.argv).toEqual([
      'grok',
      '--resume',
      'grok-session',
    ]);
    expect(planNativeResume('pi', 'pi-session')?.argv).toEqual([
      'pi',
      '--session',
      'pi-session',
    ]);
  });

  it('uses cursor-agent.cmd only on Windows', () => {
    expect(planNativeResume('cursor', 'c1', 'windows')?.argv).toEqual([
      'cursor-agent.cmd',
      '--resume',
      'c1',
    ]);
    expect(planNativeResume('cursor', 'c1', 'macos')?.argv).toEqual([
      'cursor-agent',
      '--resume',
      'c1',
    ]);
  });

  it('rejects empty, control, oversized, and unknown agents', () => {
    expect(planNativeResume('claude', '  ')).toBeNull();
    expect(planNativeResume('claude', 'bad\nid')).toBeNull();
    expect(planNativeResume('claude', 'x'.repeat(513))).toBeNull();
    expect(planNativeResume('workbuddy', 'wb')).toBeNull();
    expect(planNativeResume('dsh', 'dsh-session')).toBeNull();
  });

  it('trims whitespace around a valid id', () => {
    expect(planNativeResume('claude', '  abc  ')?.argv[2]).toBe('abc');
  });
});

describe('formatResumeCommand', () => {
  it('quotes ids that would break a shell', () => {
    expect(formatResumeCommand(['claude', '--resume', 'a;b'])).toBe(
      'claude --resume "a;b"',
    );
  });
});

describe('nativeResumeCommand', () => {
  it('returns a copyable command string', () => {
    expect(nativeResumeCommand('codex', 'sess-1')).toBe('codex resume sess-1');
  });
});
