import { describe, expect, it } from 'vitest';
import { canToggleListedPlugin } from './can-toggle';

describe('canToggleListedPlugin', () => {
  it('allows Claude and Grok listed packs only', () => {
    expect(canToggleListedPlugin('claude')).toBe(true);
    expect(canToggleListedPlugin('grok')).toBe(true);
  });

  it('hides buttons for planned and unsupported agents', () => {
    expect(canToggleListedPlugin('codex')).toBe(false);
    expect(canToggleListedPlugin('pi')).toBe(false);
    expect(canToggleListedPlugin('cursor')).toBe(false);
    expect(canToggleListedPlugin('dsh')).toBe(false);
    expect(canToggleListedPlugin('kimi')).toBe(false);
    expect(canToggleListedPlugin('workbuddy')).toBe(false);
  });
});
