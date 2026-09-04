import { describe, expect, it } from 'vitest';
import { translate } from '@/lib/i18n';
import {
  ACTIVITY_TRACE_STAGES,
  ACTIVITY_TRACE_WIDTH_SPECS,
  activityTraceColumnLabel,
  activityTraceHoverDetail,
  activityTraceInboundEndpoint,
  activityTraceInboundPath,
  activityTraceKeyParts,
  activityTraceModelLabel,
  activityTraceStageLabel,
  activityTraceStageStatusLabel,
  activityTraceUpstreamEndpoint,
  activityTraceUpstreamPath,
  formatTraceSeconds,
  formatTraceTokens,
} from './activity-trace-list-model';

const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('zh', key, params);

describe('activity-trace-list-model', () => {
  it('covers the monitoring columns and five stages', () => {
    expect(ACTIVITY_TRACE_WIDTH_SPECS.map((spec) => spec.key)).toEqual([
      'time',
      'key',
      'endpoint',
      'model',
      'firstToken',
      'duration',
      'tokens',
      'stages',
      'route',
      'details',
    ]);
    expect([...ACTIVITY_TRACE_STAGES]).toEqual([
      'local_auth',
      'pool',
      'conversion',
      'upstream_auth',
      'upstream',
    ]);
    expect(activityTraceColumnLabel('time', t)).toBe('时间');
    expect(activityTraceColumnLabel('key', t)).toBe('密钥');
    expect(activityTraceColumnLabel('endpoint', t)).toBe('端点');
    expect(activityTraceColumnLabel('model', t)).toBe('模型');
    expect(activityTraceColumnLabel('firstToken', t)).toBe('首字');
    expect(activityTraceColumnLabel('duration', t)).toBe('请求时长');
    expect(activityTraceColumnLabel('tokens', t)).toBe('Token');
    expect(activityTraceColumnLabel('stages', t)).toBe('五段');
    expect(activityTraceColumnLabel('route', t)).toBe('路由');
    expect(activityTraceColumnLabel('details', t)).toBe('详情');
    expect(activityTraceStageLabel('local_auth', t)).toBe('本机鉴权');
    expect(activityTraceStageStatusLabel('ok', t)).toBe('成功');
    expect(activityTraceStageStatusLabel('failed', t)).toBe('失败');
    expect(activityTraceStageStatusLabel('skipped', t)).toBe('未到达');
  });

  it('formats first-token and request duration in seconds', () => {
    expect(formatTraceSeconds(800, t)).toBe('0.8s');
    expect(formatTraceSeconds(4200, t)).toBe('4.2s');
    expect(formatTraceSeconds(12_000, t)).toBe('12s');
    expect(formatTraceSeconds(null, t)).toBe('');
  });

  it('formats consumed tokens and prefers the request model', () => {
    expect(formatTraceTokens(1200, 340, t)).toBe('1.2K / 340');
    expect(formatTraceTokens(null, null, t)).toBe('');
    expect(activityTraceModelLabel({ model: 'claude-sonnet' })).toBe('claude-sonnet');
    expect(activityTraceModelLabel({
      upstream: { upstreamModel: 'gpt-5' },
    })).toBe('gpt-5');
  });

  it('joins the key abbreviation with the matching token name', () => {
    expect(activityTraceKeyParts({ localAuth: { keyLast4: '1234' } }).label).toBe('••••1234');
    expect(activityTraceKeyParts(
      { localAuth: { keyLast4: '1234', profileId: 'pool-a' } },
      [
        { token: 'ahb_xxxx1234', name: 'Claude 入口', poolId: 'pool-a' },
        { token: 'ahb_yyyy1234', name: '其他', poolId: 'pool-b' },
      ],
    )).toEqual({
      abbrev: 'ahb_••••1234',
      name: 'Claude 入口',
      label: 'ahb_••••1234 Claude 入口',
    });
  });

  it('shows inbound and upstream protocol paths in the table, full URLs in details', () => {
    expect(activityTraceInboundPath({ path: '/v1/messages' })).toBe('/v1/messages');
    expect(activityTraceUpstreamPath({
      upstreamRequest: { url: 'https://api.anthropic.com/v1/messages' },
    })).toBe('/v1/messages');
    expect(activityTraceUpstreamPath({
      upstream: { url: 'https://api.openai.com/v1/chat/completions' },
    })).toBe('/v1/chat/completions');
    expect(activityTraceInboundEndpoint({
      path: '/v1/messages',
      localAuth: { port: 8787 },
    })).toBe('http://127.0.0.1:8787/v1/messages');
    expect(activityTraceUpstreamEndpoint({
      upstreamRequest: { url: 'https://api.anthropic.com/v1/messages' },
    })).toBe('https://api.anthropic.com/v1/messages');
    expect(activityTraceHoverDetail('入站 · 本地调用端点', 'http://127.0.0.1:8787/v1/messages'))
      .toBe('入站 · 本地调用端点 http://127.0.0.1:8787/v1/messages');
  });
});
