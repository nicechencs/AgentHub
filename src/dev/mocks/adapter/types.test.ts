import { describe, expect, it } from 'vitest';
import { PROTOCOL_MISMATCH_REASON, SAME_PROTOCOL_NO_EDGE_REASON } from './types';

describe('picker ineligibility copy', () => {
  it('uses short Chinese without protocol-graph jargon', () => {
    expect(PROTOCOL_MISMATCH_REASON).toBe('这份登录接不到这个 Agent。');
    expect(SAME_PROTOCOL_NO_EDGE_REASON).toBe('这条接到方式还没做好，暂不能绑定。');
    const blob = `${PROTOCOL_MISMATCH_REASON}\n${SAME_PROTOCOL_NO_EDGE_REASON}`;
    expect(blob).not.toContain('协议图');
    expect(blob).not.toContain('入口没有交集');
    expect(blob).not.toContain('协议不通');
  });
});
