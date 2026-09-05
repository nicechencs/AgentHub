import { describe, expect, it } from 'vitest';
import {
  buildPreviewTimeline,
  classifyExcerptRows,
  excerptTurnsToRecordLines,
  parseApprovalDecisions,
  splitExcerptDocument,
  splitExcerptTurns,
} from './session-excerpt';

describe('splitExcerptTurns', () => {
  it('returns empty for blank input', () => {
    expect(splitExcerptTurns('')).toEqual([]);
    expect(splitExcerptTurns('   \n  ')).toEqual([]);
  });

  it('keeps a single blob as one user turn', () => {
    expect(splitExcerptTurns('修复登录页 token 过期问题')).toEqual([
      { role: 'user', text: '修复登录页 token 过期问题' },
    ]);
  });

  it('splits legacy excerpt separators into visual turns', () => {
    expect(
      splitExcerptTurns(
        [
          '帮我检查 refresh 流程',
          '---',
          '先看登录页有没有把过期 token 清掉。',
          '---',
          '然后补一条失败用例。',
        ].join('\n'),
      ),
    ).toEqual([
      { role: 'user', text: '帮我检查 refresh 流程' },
      { role: 'assistant', text: '先看登录页有没有把过期 token 清掉。' },
      { role: 'user', text: '然后补一条失败用例。' },
    ]);
  });

  it('ignores blank pieces and normalizes CRLF in the legacy format', () => {
    expect(splitExcerptTurns('hello\r\n---\r\n\r\n---\r\nworld\n')).toEqual([
      { role: 'user', text: 'hello' },
      { role: 'assistant', text: 'world' },
    ]);
  });

  it('splits consecutive role-tagged turns used by mock excerpts', () => {
    expect(
      splitExcerptTurns(
        [
          '---turn:user---',
          '修复登录页 token 过期问题',
          '---turn:assistant---',
          '工作目录：C:\\Users\\demo\\app',
          '',
          '已按这条会话继续，下一步建议先核对现有实现再改。',
        ].join('\n'),
      ),
    ).toEqual([
      { role: 'user', text: '修复登录页 token 过期问题' },
      {
        role: 'assistant',
        text: '工作目录：C:\\Users\\demo\\app\n\n已按这条会话继续，下一步建议先核对现有实现再改。',
      },
    ]);
  });

  it('skips empty role-tagged blocks', () => {
    expect(
      splitExcerptTurns(['---turn:user---', '', '---turn:assistant---', 'ok'].join('\n')),
    ).toEqual([{ role: 'assistant', text: 'ok' }]);
  });

  it('keeps a convention document out of visual turns', () => {
    const excerpt = [
      '---doc:convention---',
      '# AGENTS.md',
      '',
      '- 日常合入 dev',
      '---turn:user---',
      '帮我看看当前界面',
      '---turn:assistant---',
      '先看连接页。',
    ].join('\n');
    expect(splitExcerptTurns(excerpt)).toEqual([
      { role: 'user', text: '帮我看看当前界面' },
      { role: 'assistant', text: '先看连接页。' },
    ]);
  });

  it('keeps markdown horizontal rules inside a role-tagged assistant turn', () => {
    const excerpt = [
      '---turn:user---',
      '把这段 markdown 预览出来',
      '---turn:assistant---',
      '## 结论',
      '',
      '先改解析。',
      '',
      '---',
      '',
      '再渲染正文。',
    ].join('\n');
    expect(splitExcerptTurns(excerpt)).toEqual([
      { role: 'user', text: '把这段 markdown 预览出来' },
      {
        role: 'assistant',
        text: '## 结论\n\n先改解析。\n\n---\n\n再渲染正文。',
      },
    ]);
  });
});

describe('splitExcerptDocument', () => {
  it('returns the convention block and conversation turns', () => {
    expect(
      splitExcerptDocument(
        [
          '---doc:convention---',
          '# 项目约定',
          '---turn:user---',
          '帮我看看当前界面',
        ].join('\n'),
      ),
    ).toEqual({
      convention: '# 项目约定',
      turns: [{ role: 'user', text: '帮我看看当前界面' }],
    });
  });

  it('attaches timestamps that precede a turn marker', () => {
    expect(
      splitExcerptDocument(
        [
          '---ts:2026-09-03T21:42:05.000Z---',
          '---turn:user---',
          '清理已经合并至dev的分支',
        ].join('\n'),
      ),
    ).toEqual({
      convention: null,
      turns: [
        {
          role: 'user',
          text: '清理已经合并至dev的分支',
          at: '2026-09-03T21:42:05.000Z',
        },
      ],
    });
  });
});

describe('buildPreviewTimeline', () => {
  it('puts project instructions first and approvals among turns by time', () => {
    const items = buildPreviewTimeline(
      '# 约定',
      [
        { role: 'user', text: '先问', at: '2026-09-03T10:00:00.000Z' },
        { role: 'assistant', text: '先答', at: '2026-09-03T10:05:00.000Z' },
        { role: 'user', text: '再问', at: '2026-09-03T10:20:00.000Z' },
      ],
      [
        {
          outcome: 'allow',
          rationale: '只读文件',
          raw: '{}',
          at: '2026-09-03T10:06:00.000Z',
        },
      ],
    );
    expect(items.map((item) => item.kind)).toEqual([
      'convention',
      'turn',
      'turn',
      'approval',
      'turn',
    ]);
  });

  it('falls back to after assistant replies when times are missing', () => {
    const items = buildPreviewTimeline(null, [
      { role: 'user', text: '问' },
      { role: 'assistant', text: '答' },
    ], [
      { outcome: 'allow', rationale: '只读文件', raw: '{}' },
    ]);
    expect(items.map((item) => item.kind)).toEqual(['turn', 'turn', 'approval']);
  });
});

describe('parseApprovalDecisions', () => {
  it('reads allow/deny JSON from assistant turns', () => {
    expect(
      parseApprovalDecisions([
        { role: 'user', text: 'ignored' },
        {
          role: 'assistant',
          text: '{"risk_level":"low","outcome":"allow","rationale":"只读本地文件"}',
        },
        {
          role: 'assistant',
          text: '{"outcome":"deny","rationale":"超出范围"}',
        },
        { role: 'assistant', text: '先看连接页。' },
      ]),
    ).toEqual([
      {
        outcome: 'allow',
        rationale: '只读本地文件',
        riskLevel: 'low',
        raw: '{"risk_level":"low","outcome":"allow","rationale":"只读本地文件"}',
      },
      {
        outcome: 'deny',
        rationale: '超出范围',
        raw: '{"outcome":"deny","rationale":"超出范围"}',
      },
    ]);
  });
});

describe('excerptTurnsToRecordLines', () => {
  it('labels user and assistant turns for a copyable record', () => {
    expect(
      excerptTurnsToRecordLines(
        [
          { role: 'user', text: '修登录' },
          { role: 'assistant', text: '先看 token。' },
        ],
        { user: '你', assistant: 'Claude' },
      ),
    ).toEqual([
      { speaker: '你', text: '修登录' },
      { speaker: 'Claude', text: '先看 token。' },
    ]);
  });
});

describe('classifyExcerptRows', () => {
  it('treats a missing id as an error (core skip-on-failure)', () => {
    expect(classifyExcerptRows('sess-a', [])).toEqual({ status: 'error' });
    expect(classifyExcerptRows('sess-a', [{ id: 'other', excerpt: 'hi' }])).toEqual({
      status: 'error',
    });
  });

  it('treats a blank body as empty', () => {
    expect(classifyExcerptRows('sess-a', [{ id: 'sess-a', excerpt: '  ' }])).toEqual({
      status: 'empty',
    });
  });

  it('returns the matching row body', () => {
    expect(
      classifyExcerptRows('sess-a', [
        { id: 'sess-b', excerpt: 'nope' },
        { id: 'sess-a', excerpt: '  hello  ' },
      ]),
    ).toEqual({ status: 'ready', excerpt: 'hello', truncated: false });
  });

  it('passes through truncated when the file was not fully read', () => {
    expect(
      classifyExcerptRows('sess-a', [{ id: 'sess-a', excerpt: 'hello', truncated: true }]),
    ).toEqual({ status: 'ready', excerpt: 'hello', truncated: true });
  });
});
