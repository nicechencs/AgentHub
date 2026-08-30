import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { localizeSkillMarketDescription, localizeStoredUiCopy } from './stored-copy';

describe('localizeStoredUiCopy', () => {
  const tEn = createTranslator('en');
  const tZh = createTranslator('zh');

  it('remaps stored Chinese health and connection labels on English', () => {
    expect(localizeStoredUiCopy('已验证', tEn)).toBe('Verified');
    expect(localizeStoredUiCopy('可续期·未验证', tEn)).toBe('Renewable');
    expect(localizeStoredUiCopy('已配置，尚未验证', tEn)).toBe('Configured');
    expect(localizeStoredUiCopy('未配置', tEn)).toBe('Not configured');
    expect(localizeStoredUiCopy('已登录', tEn)).toBe('Signed in');
    expect(localizeStoredUiCopy('本机路由', tEn)).toBe('Local route');
    expect(localizeStoredUiCopy('本机路由 · Kimi', tEn)).toBe('Local route · Kimi');
    expect(localizeStoredUiCopy('未检测登录态', tEn)).toBe('Unknown');
    expect(localizeStoredUiCopy('未配置', tEn)).not.toMatch(/[\u4e00-\u9fff]/);
  });

  it('keeps Chinese when no translator is passed', () => {
    expect(localizeStoredUiCopy('已验证')).toBe('已验证');
    expect(localizeStoredUiCopy('可续期·未验证')).toBe('可续期·未验证');
    expect(localizeStoredUiCopy('未配置', tZh)).toBe('未配置');
  });
});

describe('localizeSkillMarketDescription', () => {
  it('rewrites the skills.sh install-count line', () => {
    const tEn = createTranslator('en');
    expect(localizeSkillMarketDescription('来自 skills.sh · 12.3K 次安装', tEn)).toBe(
      'From skills.sh · 12.3K installs',
    );
    expect(localizeSkillMarketDescription('来自 skills.sh · 12.3K 次安装')).toBe(
      '来自 skills.sh · 12.3K 次安装',
    );
  });
});
