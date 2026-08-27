import { describe, expect, it, vi } from 'vitest';
import {
  createInstallProgressSubscription,
  installOutputChunksToLines,
  isSetupGuideOutcome,
  recordInstallOutputChunk,
  resolveInstallTaskStatus,
  splitInstallOutcomeDisplay,
} from './use-agent-card-lifecycle';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

describe('install progress subscription lifecycle', () => {
  it('awaits subscription setup and cleans up a listener that resolves after disposal', async () => {
    const setup = deferred<() => void>();
    const lateUnsubscribe = vi.fn();
    const subscribe = vi.fn(() => setup.promise);
    const subscription = createInstallProgressSubscription(subscribe, () => {});

    subscription.dispose();
    setup.resolve(lateUnsubscribe);
    await subscription.ready;

    expect(subscribe).toHaveBeenCalledOnce();
    expect(lateUnsubscribe).toHaveBeenCalledOnce();
  });

  it('rejects setup failures instead of silently starting the command', async () => {
    const error = new Error('listen failed');
    const subscription = createInstallProgressSubscription(
      async () => {
        throw error;
      },
      () => {},
    );

    await expect(subscription.ready).rejects.toBe(error);
  });
});

describe('install output raw chunks', () => {
  it('keeps an empty chunk instead of dropping it', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, '');
    expect(chunks).toEqual(['']);
    expect(installOutputChunksToLines(chunks)).toEqual(['']);
  });

  it('keeps whitespace and multiple newlines and joins mid-line splits', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, '   ');
    recordInstallOutputChunk(chunks, '\n');
    recordInstallOutputChunk(chunks, '\n');
    recordInstallOutputChunk(chunks, 'hel');
    recordInstallOutputChunk(chunks, 'lo\n');
    recordInstallOutputChunk(chunks, 'wor');
    recordInstallOutputChunk(chunks, 'ld');

    expect(chunks).toEqual(['   ', '\n', '\n', 'hel', 'lo\n', 'wor', 'ld']);
    expect(installOutputChunksToLines(chunks)).toEqual(['   ', '', 'hello', 'world']);
  });

  it('does not trimEnd trailing spaces on a chunk', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, 'keep  ');
    expect(installOutputChunksToLines(chunks)).toEqual(['keep  ']);
  });
});

describe('setup-guide install outcome', () => {
  it('does not treat opening the official setup page as a failed install', () => {
    const guided = {
      ok: false,
      code: 'setup_guide',
      message: 'workbuddy 已打开官网安装页，请完成安装后重启 AgentHub',
      logs: [
        '诊断：该 Agent 没有脚本安装，已打开官网安装页。请完成安装后，完全退出并重启 AgentHub。',
        '$ xdg-open https://www.codebuddy.cn/work/',
      ],
    };
    expect(isSetupGuideOutcome(guided)).toBe(true);
    expect(resolveInstallTaskStatus(guided)).toBe('guided');
    expect(guided.message).toContain('官网安装页');
    expect(guided.message).not.toContain('失败');

    const display = splitInstallOutcomeDisplay(guided);
    expect(display.diagnosis).toBe(guided.message);
    expect(display.lines[0]?.startsWith('诊断：')).toBe(true);
    expect(display.lines[0]).not.toEqual(display.diagnosis);
  });

  it('keeps real command failures as failed with diagnosis above raw output', () => {
    const failed = {
      ok: false,
      code: null,
      message: 'codex 安装失败：没有写入权限（EACCES，退出码 243）',
      logs: [
        '诊断：没有写入权限，不是 PATH 问题。',
        '$ npm install -g @openai/codex',
        'npm ERR! code EACCES',
      ],
    };
    expect(isSetupGuideOutcome(failed)).toBe(false);
    expect(resolveInstallTaskStatus(failed)).toBe('failed');
    const display = splitInstallOutcomeDisplay(failed);
    expect(display.diagnosis).toBe(failed.message);
    expect(display.lines[0]).toBe('诊断：没有写入权限，不是 PATH 问题。');
    expect(display.lines[1]?.startsWith('$ ')).toBe(true);
  });

  it('does not dump npm HTTP progress as the fail-panel body', () => {
    const logs = [
      '诊断：安装命令未成功退出（退出码 1）。',
      '$ npm install -g @openai/codex',
      ...Array.from({ length: 50 }, (_, i) => `npm http fetch GET 200 https://registry.npmjs.org/p-${i}`),
      'npm ERR! code EACCES',
    ];
    const display = splitInstallOutcomeDisplay({
      message: 'codex 安装失败（退出码 1）',
      logs,
    });
    expect(display.diagnosis).toBe('codex 安装失败（退出码 1）');
    expect(display.lines[0]?.startsWith('诊断：')).toBe(true);
    expect(display.lines.some((line) => line.includes('已省略') && line.includes('下载进度'))).toBe(
      true,
    );
    expect(display.lines.filter((line) => line.includes('http fetch')).length).toBeLessThan(3);
    expect(display.lines.some((line) => line.includes('EACCES'))).toBe(true);
  });

  it('marks ok outcomes done even if a leftover code is present', () => {
    expect(resolveInstallTaskStatus({ ok: true, code: 'setup_guide' })).toBe('done');
    expect(isSetupGuideOutcome({ ok: true, code: 'setup_guide' })).toBe(false);
  });
});

describe('install output chunk cap', () => {
  it('trims the chunk array to the cap while keeping the tail and line cap', () => {
    const chunks: string[] = [];
    for (let i = 0; i < 2500; i += 1) {
      recordInstallOutputChunk(chunks, `chunk-${i}\n`);
    }

    expect(chunks.length).toBe(2000);
    expect(chunks[0]).toBe('chunk-500\n');
    expect(chunks.at(-1)).toBe('chunk-2499\n');

    const lines = installOutputChunksToLines(chunks);
    expect(lines.length).toBeLessThanOrEqual(400);
    // Trailing '' from the final newline is part of the last-400 window.
    expect(lines[0]).toBe('chunk-2101');
    expect(lines.at(-2)).toBe('chunk-2499');
  });

  it('does not lose mid-line content across a head trim', () => {
    const chunks: string[] = [];
    recordInstallOutputChunk(chunks, 'head-that-will-be-trimmed ');
    for (let i = 0; i < 2000; i += 1) {
      recordInstallOutputChunk(chunks, `\nrow-${i}`);
    }
    // This chunk splits a line whose start lives in an already-retained
    // chunk; joining must still produce complete rows without duplication.
    recordInstallOutputChunk(chunks, '-suffix\n');
    expect(chunks.length).toBe(2000);
    expect(chunks[0]).toBe('\nrow-1');

    const lines = installOutputChunksToLines(chunks);
    expect(lines.at(-2)).toBe(`row-${1999}-suffix`);
    // No retained row was split by the head trim (the trailing '' comes from
    // the final newline and is expected).
    expect(lines.some((line) => line === '-suffix')).toBe(false);
  });
});
