import { beforeEach, describe, expect, it } from 'vitest';
import { createBackend } from './create-backend';
import { resetMockAgentVisibility } from './agent';

describe('mock agent visibility', () => {
  beforeEach(() => {
    resetMockAgentVisibility();
  });

  it('stamps hidden after setAgentHidden and reset clears it', async () => {
    const backend = createBackend();
    await backend.agent.setAgentHidden('claude', true);
    const hidden = await backend.agent.getAgent('claude');
    expect(hidden.hidden).toBe(true);

    await backend.agent.setAgentHidden('claude', false);
    expect((await backend.agent.getAgent('claude')).hidden).toBe(false);

    await backend.agent.setAgentHidden('claude', true);
    resetMockAgentVisibility();
    const backend2 = createBackend();
    expect((await backend2.agent.getAgent('claude')).hidden).toBe(false);
  }, 15_000);
});
