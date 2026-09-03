import { describe, expect, it } from 'vitest';

import { canonicalUsageModel, usageModelsMatch } from './usage-model';

describe('canonicalUsageModel', () => {
  it('treats Grok -build / prefix ids as the public model name', () => {
    expect(canonicalUsageModel('grok-4.6-build')).toBe('grok-4.6');
    expect(canonicalUsageModel('grok-4.6')).toBe('grok-4.6');
    expect(canonicalUsageModel('[grok] grok-4.6-build')).toBe('grok-4.6');
    expect(canonicalUsageModel('xai/grok-4.6-build')).toBe('grok-4.6');
  });

  it('leaves non-Grok names alone', () => {
    expect(canonicalUsageModel('claude-opus-4')).toBe('claude-opus-4');
    expect(canonicalUsageModel('my-build')).toBe('my-build');
  });
});

describe('usageModelsMatch', () => {
  it('matches grok-4.6 against grok-4.6-build', () => {
    expect(usageModelsMatch('grok-4.6-build', 'grok-4.6')).toBe(true);
    expect(usageModelsMatch('grok-4.6', 'grok-4.6-build')).toBe(true);
    expect(usageModelsMatch('opus', 'all')).toBe(true);
    expect(usageModelsMatch('opus', 'sonnet')).toBe(false);
  });
});
