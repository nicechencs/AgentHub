import type { AgentSession } from '@/lib/types';
import { splitExcerptTurns } from './session-excerpt';

/** Keep continue/summarize payloads well under typical sessionStorage quota. */
export const CONTINUE_RECORD_CHAR_LIMIT = 400_000;

export type ContinueRecord = {
  excerpt: string;
  truncated?: boolean;
};

export function buildSummaryPrompt(
  agentName: string,
  excerpts: { title: string; cwd?: string | null; updatedAt: string; excerpt: string; truncated?: boolean }[],
): string {
  const blocks = excerpts.map((e, i) => {
    const head = [
      `### 记录 ${i + 1}: ${e.title}`,
      e.cwd ? `工作目录: ${e.cwd}` : null,
      `更新时间: ${e.updatedAt}`,
      e.truncated ? '（文件较大，中间有一段没有读完）' : null,
      '',
      e.excerpt || '（没有可读的对话）',
    ]
      .filter(Boolean)
      .join('\n');
    return head;
  });
  return [
    `请根据以下 ${excerpts.length} 条 ${agentName} 历史会话，写一份结构化总结。`,
    '',
    '要求：',
    '1. 每条记录的核心目标与结论',
    '2. 跨记录的共同主题或重复问题',
    '3. 未完成事项与建议下一步',
    '4. 若信息不足请明确标出，不要编造',
    '',
    '---',
    '',
    blocks.join('\n\n---\n\n'),
  ].join('\n');
}

export function buildContinuePrompt(
  p: AgentSession,
  record?: ContinueRecord | null,
  charLimit = CONTINUE_RECORD_CHAR_LIMIT,
): string {
  const bits = [
    '我想基于这条历史会话继续工作。',
    p.cwd ? `工作目录：${p.cwd}` : null,
    formatContinueRecord(p, record, charLimit),
    '',
    '请先简要回顾你认为的上下文（若不确定请说明），然后问我下一步要做什么。',
  ];
  return bits.filter(Boolean).join('\n');
}

export function formatContinueRecord(
  p: AgentSession,
  record?: ContinueRecord | null,
  charLimit = CONTINUE_RECORD_CHAR_LIMIT,
): string {
  const excerpt = record?.excerpt?.trim() ?? '';
  if (!excerpt) {
    return p.preview ? `上次话题预览：${p.preview}` : `标题：${p.title}`;
  }
  const turns = splitExcerptTurns(excerpt);
  const body =
    turns.length > 0
      ? turns
          .map((turn) => `${turn.role === 'user' ? '你' : '助手'}：\n${turn.text}`)
          .join('\n\n')
      : excerpt;
  const notes: string[] = [];
  if (record?.truncated) {
    notes.push('文件较大，中间有一段没有读完。下面是开头和最近的对话。');
  }
  const noteLen = notes.join('\n').length;
  const fitted = fitContinueRecord(body, Math.max(80, charLimit - noteLen));
  if (fitted.trimmed) {
    notes.push('对话很长，继续提示里只放了开头和结尾。');
  }
  const head = notes.length > 0 ? `${notes.join('\n')}\n\n` : '';
  return `${head}对话记录：\n${fitted.text}`;
}

export function fitContinueRecord(
  text: string,
  limit: number,
): { text: string; trimmed: boolean } {
  if (limit <= 0) return { text: '', trimmed: text.length > 0 };
  if (text.length <= limit) return { text, trimmed: false };
  const marker = '\n\n…（中间部分未放入继续提示）\n\n';
  if (limit <= marker.length + 20) {
    return { text: text.slice(0, limit), trimmed: true };
  }
  const keep = Math.max(10, Math.floor((limit - marker.length) / 2));
  return {
    text: `${text.slice(0, keep)}${marker}${text.slice(-keep)}`,
    trimmed: true,
  };
}
