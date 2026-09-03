import { describe, expect, it } from 'vitest';
import { installProgressChunk, isProgressForAgent } from './install-types';

describe('isProgressForAgent', () => {
  it('matches only the given agent id', () => {
    expect(isProgressForAgent({ agentId: 'claude', chunk: 'ok' }, 'claude')).toBe(true);
    expect(isProgressForAgent({ agentId: 'codex', chunk: 'ok' }, 'claude')).toBe(false);
  });

  it('rejects runtime-only chunks with a null or missing agentId', () => {
    expect(isProgressForAgent({ agentId: null, action: 'runtime', chunk: 'ok' }, 'claude')).toBe(
      false,
    );
    expect(isProgressForAgent({ chunk: 'ok' }, 'claude')).toBe(false);
  });
});

describe('installProgressChunk', () => {
  it('returns chunk text and empty string when chunk is missing', () => {
    expect(installProgressChunk({ chunk: 'hel' })).toBe('hel');
    expect(installProgressChunk({ chunk: '' })).toBe('');
    expect(installProgressChunk({} as { chunk: string })).toBe('');
  });
});
