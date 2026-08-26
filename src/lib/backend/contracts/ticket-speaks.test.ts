import { describe, expect, it } from 'vitest';
import type { TicketSurface } from './ticket';
import { TICKET_SURFACE_SPEAKS, ticketSurfaceSpeaks } from './ticket-speaks';

const SURFACES: TicketSurface[] = [
  'kimi-code-membership',
  'anthropic-api',
  'openai-api',
  'xai-api',
  'glm-coding-plan',
  'deepseek-api',
  'codex-chatgpt-subscription',
  'claude-subscription',
  'grok-xai-subscription',
  'unknown',
];

describe('ticket-speaks fixture', () => {
  it('covers every surface and matches GLM/DeepSeek openai-responses', () => {
    expect(Object.keys(TICKET_SURFACE_SPEAKS).sort()).toEqual([...SURFACES].sort());
    expect(ticketSurfaceSpeaks('glm-coding-plan')).toEqual([
      'anthropic-messages',
      'openai-chat',
      'openai-responses',
    ]);
    expect(ticketSurfaceSpeaks('deepseek-api')).toEqual(ticketSurfaceSpeaks('glm-coding-plan'));
    expect(ticketSurfaceSpeaks('unknown')).toEqual([]);
  });
});
