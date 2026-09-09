import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  chatEffortHint,
  chatEffortLabel,
  chatModShiftIShouldOpenModel,
  chatModelDisplayName,
} from './chat-model-labels';

const zh = createTranslator('zh');
const en = createTranslator('en');

describe('chatModelDisplayName', () => {
  it('turns catalog ids into readable names without exposing the raw id', () => {
    expect(chatModelDisplayName('gpt-5.3-codex-spark')).toBe('GPT 5.3 Codex Spark');
    expect(chatModelDisplayName('gpt-5.6-sol')).toBe('GPT 5.6 Sol');
    expect(chatModelDisplayName('gpt-mock')).toBe('GPT Mock');
    expect(chatModelDisplayName('grok-4.6')).toBe('Grok 4.6');
    expect(chatModelDisplayName('grok-code-fast-1')).toBe('Grok Code Fast 1');
    expect(chatModelDisplayName('claude-sonnet-4.5')).toBe('Claude Sonnet 4.5');
    expect(chatModelDisplayName('claude-haiku-4.5')).toBe('Claude Haiku 4.5');
    expect(chatModelDisplayName('o3')).toBe('O3');
  });

  it('translates the auto slot', () => {
    expect(chatModelDisplayName('auto')).toBe('auto');
    expect(chatModelDisplayName('auto', zh)).toBe('自动');
    expect(chatModelDisplayName('Auto', en)).toBe('Auto');
  });

  it('keeps empty and unknown tokens readable', () => {
    expect(chatModelDisplayName('')).toBe('');
    expect(chatModelDisplayName('  ')).toBe('');
    expect(chatModelDisplayName('custom-router-v2')).toBe('Custom Router V2');
  });
});

describe('chatEffortLabel / chatEffortHint', () => {
  it('names known thinking levels and adds a short wait hint', () => {
    expect(chatEffortLabel('low', zh)).toBe('低');
    expect(chatEffortHint('low', zh)).toBe('更快');
    expect(chatEffortLabel('medium', zh)).toBe('中');
    expect(chatEffortHint('medium', zh)).toBe('均衡');
    expect(chatEffortLabel('high', zh)).toBe('高');
    expect(chatEffortHint('high', zh)).toBe('可能更慢');
    expect(chatEffortLabel('xhigh', zh)).toBe('很高');
    expect(chatEffortHint('xhigh', zh)).toBe('更慢');
    expect(chatEffortLabel('max', zh)).toBe('最高');
    expect(chatEffortHint('max', zh)).toBe('更慢');
  });

  it('keeps English copy short', () => {
    expect(chatEffortLabel('high', en)).toBe('High');
    expect(chatEffortHint('high', en)).toBe('May be slower');
    expect(chatEffortLabel('xhigh', en)).toBe('Extra high');
  });

  it('does not invent a hint for unknown efforts', () => {
    expect(chatEffortLabel('turbo', zh)).toBe('Turbo');
    expect(chatEffortHint('turbo', zh)).toBeNull();
  });
});

describe('chatModShiftIShouldOpenModel', () => {
  const base = {
    key: 'i',
    metaKey: false,
    ctrlKey: true,
    altKey: false,
    shiftKey: true,
    overlayOpen: false,
  };

  it('opens the model menu with Ctrl/Cmd+Shift+I', () => {
    expect(chatModShiftIShouldOpenModel(base)).toBe(true);
    expect(chatModShiftIShouldOpenModel({ ...base, ctrlKey: false, metaKey: true })).toBe(true);
    expect(chatModShiftIShouldOpenModel({ ...base, key: 'I' })).toBe(true);
  });

  it('yields to overlays, Alt, and missing modifiers', () => {
    expect(chatModShiftIShouldOpenModel({ ...base, overlayOpen: true })).toBe(false);
    expect(chatModShiftIShouldOpenModel({ ...base, altKey: true })).toBe(false);
    expect(chatModShiftIShouldOpenModel({ ...base, shiftKey: false })).toBe(false);
    expect(chatModShiftIShouldOpenModel({ ...base, ctrlKey: false, metaKey: false })).toBe(false);
    expect(chatModShiftIShouldOpenModel({ ...base, key: 'k' })).toBe(false);
  });
});
