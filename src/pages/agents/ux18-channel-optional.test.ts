import { describe, expect, it } from 'vitest';
import { en } from '@/lib/i18n/locales/en';
import { zh } from '@/lib/i18n/locales/zh';
import { missingChannelStatusKey } from './agent-detail-model';

describe('UX18 channel optional status', () => {
  it('maps installed primary to optional alternate copy', () => {
    expect(missingChannelStatusKey({ agentInstalled: false })).toBe('agents.card.notInstalled');
    expect(missingChannelStatusKey({ agentInstalled: true })).toBe('agents.card.channelOptional');
    expect(missingChannelStatusKey({ agentInstalled: true, linuxUnsupported: true })).toBe(
      'agents.card.linuxUnsupported',
    );
    expect(en.agents.card.channelOptional.toLowerCase()).toContain('optional');
    expect(en.agents.card.channelOptional.toLowerCase()).toContain('not selected');
    expect(zh.agents.card.channelOptional).toContain('未选用');
    expect(zh.agents.card.channelOptional).toContain('可选');
    expect(zh.agents.card.channelOptional.includes(zh.agents.card.notInstalled)).toBe(false);
  });
});
