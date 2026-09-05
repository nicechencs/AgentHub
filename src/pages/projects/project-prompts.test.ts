import { describe, expect, it } from 'vitest';
import type { AgentSession } from '@/lib/types';
import {
  buildContinuePrompt,
  buildSummaryPrompt,
  fitContinueRecord,
  formatContinueRecord,
} from './project-prompts';

function session(partial: Partial<AgentSession> = {}): AgentSession {
  return {
    id: 'claude:projects/app/sess.jsonl',
    projectId: 'claude:proj:app',
    agentId: 'claude',
    title: '修登录',
    path: 'C:\\Users\\demo\\.claude\\projects\\app\\sess.jsonl',
    relativePath: 'projects/app/sess.jsonl',
    sizeBytes: 1024,
    updatedAt: '2026-09-05T00:00:00.000Z',
    preview: '上次话题预览字',
    cwd: 'C:\\Users\\demo\\app',
    sessionId: 'sess',
    ...partial,
  };
}

describe('buildContinuePrompt', () => {
  it('falls back to the list preview when no record is loaded', () => {
    const prompt = buildContinuePrompt(session());
    expect(prompt).toContain('工作目录：C:\\Users\\demo\\app');
    expect(prompt).toContain('上次话题预览：上次话题预览字');
    expect(prompt).not.toContain('对话记录：');
  });

  it('carries every loaded turn, not the 120-character list preview', () => {
    const excerpt = [
      '---turn:user---',
      '第一轮用户发言，足够长，不会被列表预览截成 120 字。'.repeat(3),
      '---turn:assistant---',
      '第一轮助手回复。',
      '---turn:user---',
      '第二轮用户发言。',
      '---turn:assistant---',
      '第二轮助手回复。',
    ].join('\n');
    const prompt = buildContinuePrompt(session(), { excerpt });
    expect(prompt).toContain('对话记录：');
    expect(prompt).toContain('第一轮用户发言');
    expect(prompt).toContain('第二轮助手回复。');
    expect(prompt).not.toContain('上次话题预览');
  });

  it('does not put project instructions into the continue prompt', () => {
    const excerpt = [
      '---doc:convention---',
      '# AGENTS.md',
      '日常合入 dev',
      '---turn:user---',
      '帮我看看当前界面',
      '---turn:assistant---',
      '先看连接页。',
    ].join('\n');
    const prompt = buildContinuePrompt(session(), { excerpt });
    expect(prompt).toContain('帮我看看当前界面');
    expect(prompt).not.toContain('日常合入 dev');
    expect(prompt).not.toContain('---doc:convention---');
  });

  it('notes when the file was not fully read', () => {
    const prompt = buildContinuePrompt(session(), {
      excerpt: '---turn:user---\nhello\n---turn:assistant---\nworld',
      truncated: true,
    });
    expect(prompt).toContain('中间有一段没有读完');
    expect(prompt).toContain('hello');
    expect(prompt).toContain('world');
  });
});

describe('fitContinueRecord', () => {
  it('keeps short text intact', () => {
    expect(fitContinueRecord('abc', 10)).toEqual({ text: 'abc', trimmed: false });
  });

  it('keeps the start and end of a long record', () => {
    const long = `START${'x'.repeat(400)}END`;
    const fitted = fitContinueRecord(long, 120);
    expect(fitted.trimmed).toBe(true);
    expect(fitted.text.startsWith('START')).toBe(true);
    expect(fitted.text.endsWith('END')).toBe(true);
    expect(fitted.text).toContain('中间部分未放入继续提示');
    expect(fitted.text.length).toBeLessThanOrEqual(120);
  });
});

describe('formatContinueRecord', () => {
  it('uses the title when preview and excerpt are both missing', () => {
    expect(formatContinueRecord(session({ preview: null, title: '无预览' }))).toBe(
      '标题：无预览',
    );
  });
});

describe('buildSummaryPrompt', () => {
  it('includes truncated records instead of calling them 摘录', () => {
    const prompt = buildSummaryPrompt('Claude', [
      {
        title: 'sess',
        cwd: 'C:\\demo',
        updatedAt: 't0',
        excerpt: '---turn:user---\nhi',
        truncated: true,
      },
    ]);
    expect(prompt).toContain('历史会话');
    expect(prompt).not.toContain('历史会话摘录');
    expect(prompt).toContain('中间有一段没有读完');
  });
});
