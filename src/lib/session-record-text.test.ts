import { describe, expect, it } from 'vitest';
import { formatSessionRecordText } from './session-record-text';

describe('formatSessionRecordText', () => {
  it('joins speaker and body with a blank line between turns', () => {
    expect(
      formatSessionRecordText([
        { speaker: '你', text: '修登录' },
        { speaker: 'Claude', text: '先看 token。' },
      ]),
    ).toBe('你\n修登录\n\nClaude\n先看 token。');
  });

  it('drops blank turns and trims bodies', () => {
    expect(
      formatSessionRecordText([
        { speaker: '你', text: '  hi  ' },
        { speaker: 'Claude', text: '   ' },
        { speaker: '', text: 'orphan' },
      ]),
    ).toBe('你\nhi\n\norphan');
  });
});
