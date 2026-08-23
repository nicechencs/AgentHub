import { describe, expect, it } from 'vitest';
import { truncateAtWord } from './text-truncate';

describe('truncateAtWord', () => {
  it('returns the original string when it already fits', () => {
    expect(truncateAtWord('本机路由', 8)).toBe('本机路由');
  });

  it('drops the last · segment instead of cutting mid-email', () => {
    expect(truncateAtWord('本机路由 · cunser@example.com', 12)).toBe('本机路由');
    expect(truncateAtWord('本机路由 · cunser@example.com', 12)).not.toContain('cunse');
  });

  it('does not cut mid-word or mid-email when there is no separator', () => {
    expect(truncateAtWord('cunser@example.com', 8)).toBe('');
    expect(truncateAtWord('compatible-route-name', 10)).toBe('');
  });

  it('drops the last path segment rather than cutting a word', () => {
    expect(truncateAtWord('relay.example.com/v1/compat', 22)).toBe('relay.example.com/v1');
  });

  it('drops the last host label rather than cutting mid-label', () => {
    expect(truncateAtWord('very-long-subdomain-name.relay.example.com', 33)).toBe(
      'very-long-subdomain-name.relay',
    );
  });

  it('does not cut mid-CJK-word when a separator exists', () => {
    expect(truncateAtWord('本机路由 额外说明文字', 6)).toBe('本机路由');
  });
});
