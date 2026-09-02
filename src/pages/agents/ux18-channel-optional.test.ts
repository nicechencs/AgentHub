import { describe, expect, it } from 'vitest';
import { en } from '@/lib/i18n/locales/en';
import { zh } from '@/lib/i18n/locales/zh';
import { missingChannelStatusKey } from './agent-detail-model';

describe('UX18 channel optional status', () => {
  it('maps installed primary to optional alternate copy', () => {
    expect(missingChannelStatusKey({ agentInstalled: false })).toBe('agents.card.notInstalled');
    expect(missingChannelStatusKey({ agentInstalled: true })).toBe('agents.card.channelOptional');
    expect(en.agents.card.channelOptional.toLowerCase()).toContain('optional');
    expect(zh.agents.card.channelOptional.includes(zh.agents.card.notInstalled)).toBe(false);
  });
});
