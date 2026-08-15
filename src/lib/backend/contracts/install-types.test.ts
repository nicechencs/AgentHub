import { describe, expect, it } from 'vitest';
import { isProgressForAgent } from './install-types';

describe('isProgressForAgent', () => {
  it('matches only the given agent id', () => {
    expect(isProgressForAgent({ agentId: 'claude', line: 'ok' }, 'claude')).toBe(true);
    expect(isProgressForAgent({ agentId: 'codex', line: 'ok' }, 'claude')).toBe(false);
  });

  it('rejects runtime-only lines with a null or missing agentId', () => {
    expect(isProgressForAgent({ agentId: null, action: 'runtime', line: 'ok' }, 'claude')).toBe(
      false,
    );
    expect(isProgressForAgent({ line: 'ok' }, 'claude')).toBe(false);
  });
});
