import { describe, expect, it } from 'vitest';
import { pathTailLabel, previewHeaderParts, splitFileLabel } from './file-name-label';

describe('splitFileLabel', () => {
  it('keeps one line: directory prefix + file name', () => {
    expect(splitFileLabel('~/.workbuddy/models.json', 'models.json')).toEqual({
      directory: '~/.workbuddy/',
      fileName: 'models.json',
    });
    expect(splitFileLabel('~/.claude/settings.json', 'settings.json')).toEqual({
      directory: '~/.claude/',
      fileName: 'settings.json',
    });
    expect(splitFileLabel('C:\\Users\\me\\.grok\\config.toml', 'config.toml')).toEqual({
      directory: 'C:\\Users\\me\\.grok\\',
      fileName: 'config.toml',
    });
  });

  it('keeps the tail of a long folder path and marks the dropped head', () => {
    expect(pathTailLabel('D:\\demo\\chen\\2026\\AgentHub')).toBe('…\\2026\\AgentHub');
    expect(pathTailLabel('/home/user/proj/src/pages/chat')).toBe('…/pages/chat');
    expect(pathTailLabel('D:/demo/chen/2026/AgentHub')).toBe('…\\2026\\AgentHub');
  });

  it('leaves a short path whole, with or without a drive', () => {
    expect(pathTailLabel('D:\\projects\\demo')).toBe('D:\\projects\\demo');
    expect(pathTailLabel('docs/architecture')).toBe('docs/architecture');
    expect(pathTailLabel('D:\\projects')).toBe('D:\\projects');
    expect(pathTailLabel('D:\\')).toBe('D:\\');
    expect(pathTailLabel('/')).toBe('/');
  });

  it('honors a custom tail width and ignores blank paths', () => {
    expect(pathTailLabel('D:\\demo\\chen\\2026\\AgentHub', 1)).toBe('…\\AgentHub');
    expect(pathTailLabel('D:\\demo\\chen\\2026\\AgentHub', 3)).toBe('…\\chen\\2026\\AgentHub');
    expect(pathTailLabel('')).toBe('');
    expect(pathTailLabel('   ')).toBe('');
  });

  it('keeps the filename first and a tail-first directory for preview chrome', () => {
    expect(previewHeaderParts('/workspace/qa-codex-filechange-scratch/probe.txt')).toEqual({
      fileName: 'probe.txt',
      directory: '/workspace/qa-codex-filechange-scratch/',
      directoryLabel: '…/qa-codex-filechange-scratch',
      fullPath: '/workspace/qa-codex-filechange-scratch/probe.txt',
    });
    expect(previewHeaderParts('D:\\demo\\chen\\2026\\AgentHub\\src\\app.ts', 'app.ts')).toEqual({
      fileName: 'app.ts',
      directory: 'D:\\demo\\chen\\2026\\AgentHub\\src\\',
      directoryLabel: '…\\src',
      fullPath: 'D:\\demo\\chen\\2026\\AgentHub\\src\\app.ts',
    });
    expect(previewHeaderParts('README.md')).toEqual({
      fileName: 'README.md',
      directory: '',
      directoryLabel: '',
      fullPath: 'README.md',
    });
  });

  it('falls back to the last path segment when the name is missing', () => {
    expect(splitFileLabel('~/.grok/auth.json', '')).toEqual({
      directory: '~/.grok/',
      fileName: 'auth.json',
    });
    expect(splitFileLabel('~/.grok/auth.json')).toEqual({
      directory: '~/.grok/',
      fileName: 'auth.json',
    });
    expect(splitFileLabel('', 'models.json')).toEqual({
      directory: '',
      fileName: 'models.json',
    });
  });
});
