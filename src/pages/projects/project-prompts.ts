import type { AgentSession } from '@/lib/types';

export function buildSummaryPrompt(
  agentName: string,
  excerpts: { title: string; cwd?: string | null; updatedAt: string; excerpt: string }[],
): string {
  const blocks = excerpts.map((e, i) => {
    const head = [
      `### 记录 ${i + 1}: ${e.title}`,
      e.cwd ? `工作目录: ${e.cwd}` : null,
      `更新时间: ${e.updatedAt}`,
      '',
      e.excerpt || '（无正文摘录）',
    ]
      .filter(Boolean)
      .join('\n');
    return head;
  });
  return [
    `请根据以下 ${excerpts.length} 条 ${agentName} 历史会话摘录，写一份结构化总结。`,
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

export function buildContinuePrompt(p: AgentSession): string {
  const bits = [
    '我想基于这条历史会话继续工作。',
    p.cwd ? `工作目录：${p.cwd}` : null,
    p.preview ? `上次话题预览：${p.preview}` : `标题：${p.title}`,
    '',
    '请先简要回顾你认为的上下文（若不确定请说明），然后问我下一步要做什么。',
  ];
  return bits.filter(Boolean).join('\n');
}
