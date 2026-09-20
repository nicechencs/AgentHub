import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it, vi } from 'vitest';
import {
  consumePendingOpenChatCwd,
  createConversationCwd,
  folderNameFromCwd,
  newChatCwdArg,
  shellOpenChatBootstrap,
  shellOpenChatHref,
} from './open-chat-cwd';

describe('folderNameFromCwd', () => {
  it('uses the last folder name on Windows and POSIX paths', () => {
    expect(folderNameFromCwd('D:\\work\\AgentHub')).toBe('AgentHub');
    expect(folderNameFromCwd('D:\\work\\AgentHub\\')).toBe('AgentHub');
    expect(folderNameFromCwd('/Users/demo/src')).toBe('src');
  });

  it('skips a trailing current-dir component from Explorer quoting', () => {
    expect(folderNameFromCwd('C:\\.')).toBe('C:');
    expect(folderNameFromCwd('D:\\work\\app\\.')).toBe('app');
  });

  it('returns empty for blank input', () => {
    expect(folderNameFromCwd('   ')).toBe('');
  });
});

describe('newChatCwdArg', () => {
  it('keeps a folder path or explicit null and drops click events', () => {
    expect(newChatCwdArg('/workspace')).toBe('/workspace');
    expect(newChatCwdArg(null)).toBeNull();
    expect(newChatCwdArg(undefined)).toBeUndefined();
    const cyclic: { target?: unknown } = {};
    cyclic.target = cyclic;
    expect(() => JSON.stringify(cyclic)).toThrow(/circular|cyclic/i);
    expect(newChatCwdArg(cyclic)).toBeUndefined();
    expect(() => JSON.stringify({
      agentIds: ['grok'],
      cwd: createConversationCwd(cyclic),
    })).not.toThrow();
    expect(createConversationCwd(cyclic)).toBeNull();
    expect(createConversationCwd('/workspace')).toBe('/workspace');
    expect(createConversationCwd(null)).toBeNull();
  });
});

describe('consumePendingOpenChatCwd', () => {
  it('is a no-op when takePending returns nothing', async () => {
    const applyBootstrap = vi.fn();
    const navigate = vi.fn();
    await expect(
      consumePendingOpenChatCwd({
        takePending: async () => null,
        applyBootstrap,
        navigate,
      }),
    ).resolves.toBe(false);
    expect(applyBootstrap).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  });

  it('writes bootstrap from takePending and navigates once', async () => {
    const applyBootstrap = vi.fn(() => true);
    const navigate = vi.fn();
    await expect(
      consumePendingOpenChatCwd({
        takePending: async () => 'D:\\work\\app',
        applyBootstrap,
        navigate,
        now: 42,
      }),
    ).resolves.toBe(true);
    expect(applyBootstrap).toHaveBeenCalledWith(shellOpenChatBootstrap('D:\\work\\app'));
    expect(navigate).toHaveBeenCalledWith(shellOpenChatHref(42));
    expect(navigate).toHaveBeenCalledTimes(1);
  });

  it('does not navigate when bootstrap cannot be stored', async () => {
    const navigate = vi.fn();
    await expect(
      consumePendingOpenChatCwd({
        takePending: async () => 'D:\\work\\app',
        applyBootstrap: () => false,
        navigate,
      }),
    ).resolves.toBe(false);
    expect(navigate).not.toHaveBeenCalled();
  });
});

describe('App open-chat wiring', () => {
  it('wakes on the event and consumes pending from one place', () => {
    const app = readFileSync(
      path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../App.tsx'),
      'utf8',
    );
    expect(app).toContain('consumePendingOpenChatCwd');
    expect(app).toContain('takePendingOpenChatCwd');
    expect(app).toContain('subscribeOpenChatCwdWakeups');
    expect(app).toContain('wake-ups only');
    expect(app).not.toMatch(/onOpenChatCwd\(\s*openFolder\s*\)/);
    expect(app).toMatch(
      /HashRouter `useNavigate` changes identity with pathname; do not resubscribe\.\s*\n\s*\}, \[\]\);/,
    );
  });

  it('omits non-string cwd before create-conversation persist and IPC', () => {
    const dir = path.dirname(fileURLToPath(import.meta.url));
    const api = readFileSync(path.resolve(dir, 'api/chat.ts'), 'utf8');
    const tauri = readFileSync(path.resolve(dir, 'backend/tauri/chat.ts'), 'utf8');
    expect(api).toContain('createConversationCwd');
    expect(tauri).toContain('createConversationCwd');
    expect(tauri).toContain('cwd: createConversationCwd(cwd)');
  });
});
