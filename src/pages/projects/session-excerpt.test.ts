import { describe, expect, it } from 'vitest';
import { classifyExcerptRows, splitExcerptTurns } from './session-excerpt';

describe('splitExcerptTurns', () => {
  it('returns empty for blank input', () => {
    expect(splitExcerptTurns('')).toEqual([]);
    expect(splitExcerptTurns('   \n  ')).toEqual([]);
  });

  it('keeps a single blob as one turn', () => {
    expect(splitExcerptTurns('修复登录页 token 过期问题')).toEqual([
      '修复登录页 token 过期问题',
    ]);
  });

  it('splits core excerpt separators into visual turns', () => {
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
      '帮我检查 refresh 流程',
      '先看登录页有没有把过期 token 清掉。',
      '然后补一条失败用例。',
    ]);
  });

  it('ignores blank pieces and normalizes CRLF', () => {
    expect(splitExcerptTurns('hello\r\n---\r\n\r\n---\r\nworld\n')).toEqual([
      'hello',
      'world',
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
    ).toEqual({ status: 'ready', excerpt: 'hello' });
  });
});
