import { describe, expect, it } from 'vitest';
import { splitFileLabel } from './file-name-label';

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
