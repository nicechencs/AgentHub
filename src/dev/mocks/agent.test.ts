import { beforeEach, describe, expect, it } from 'vitest';
import { createBackend } from './create-backend';
import { resetMockAgentVisibility } from './agent';

describe('mock agent visibility', () => {
  beforeEach(() => {
    resetMockAgentVisibility();
  });

  it('stamps hidden after setAgentHidden and reset clears ad-hoc hides but keeps store-stamp', async () => {
    const backend = createBackend();
    expect((await backend.agent.getAgent('cursor')).hidden).toBe(true);

    await backend.agent.setAgentHidden('claude', true);
    const hidden = await backend.agent.getAgent('claude');
    expect(hidden.hidden).toBe(true);

    await backend.agent.setAgentHidden('claude', false);
    expect((await backend.agent.getAgent('claude')).hidden).toBe(false);

    await backend.agent.setAgentHidden('claude', true);
    resetMockAgentVisibility();
    const backend2 = createBackend();
    expect((await backend2.agent.getAgent('claude')).hidden).toBe(false);
    expect((await backend2.agent.getAgent('cursor')).hidden).toBe(true);
  }, 15_000);
});
