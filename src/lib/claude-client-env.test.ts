import { describe, expect, it } from 'vitest';
import {
  CLAUDE_WINDOW_1M,
  claudeContextWindowFor,
  formatClaudeContextWindow,
  parseContextWindowChoice,
  stripClaudeContextMarker,
} from './claude-client-env';

describe('claude-client-env', () => {
  it('strips [1m] and only infers 1M from the marker or an override', () => {
    expect(stripClaudeContextMarker('stealth/ox-alpha[1m]')).toBe('stealth/ox-alpha');
    expect(claudeContextWindowFor('stealth/ox-alpha')).toBeNull();
    expect(claudeContextWindowFor('custom/unknown')).toBeNull();
    expect(claudeContextWindowFor('any/id[1m]')).toBe(CLAUDE_WINDOW_1M);
    expect(claudeContextWindowFor('stealth/ox-alpha', CLAUDE_WINDOW_1M)).toBe(CLAUDE_WINDOW_1M);
    expect(formatClaudeContextWindow(CLAUDE_WINDOW_1M)).toBe('1M');
    expect(parseContextWindowChoice('1048576')).toBe('1048576');
    expect(parseContextWindowChoice('1000000')).toBe('1048576');
    expect(parseContextWindowChoice('200000')).toBe('200000');
    expect(parseContextWindowChoice('')).toBe('auto');
    expect(parseContextWindowChoice('128000')).toBe('auto');
  });
});
