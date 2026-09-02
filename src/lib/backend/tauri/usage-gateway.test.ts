import { beforeEach, describe, expect, it, vi } from 'vitest';
import { gatewayUsageOverview, gatewayUsageQuery } from './usage';

const invokeMock = vi.fn();
vi.mock('./invoke', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

beforeEach(() => invokeMock.mockReset());

describe('Tauri gateway usage wrappers', () => {
  it('forwards gateway_usage_query with null-filled filters', async () => {
    invokeMock.mockResolvedValueOnce([
      {
        requestId: 'req-1',
        ts: '2026-08-30T10:00:00+00:00',
        profileId: 'profile-a',
        surface: 'responses',
        upstreamChannel: 'openai_chat',
        inputTokens: 11,
        outputTokens: 4,
        status: 'ok',
      },
    ]);
    const rows = await gatewayUsageQuery({
      since: '2026-08-30T00:00:00+00:00',
      profileId: 'profile-a',
      limit: 50,
    });

    expect(invokeMock).toHaveBeenCalledWith('gateway_usage_query', {
      since: '2026-08-30T00:00:00+00:00',
      until: null,
      profileId: 'profile-a',
      limit: 50,
    });
    expect(rows[0]).toMatchObject({
      requestId: 'req-1',
      profileId: 'profile-a',
      inputTokens: 11,
      outputTokens: 4,
      status: 'ok',
    });
  });

  it('forwards an empty gateway_usage_query filter as all-null args', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await gatewayUsageQuery();

    expect(invokeMock).toHaveBeenCalledWith('gateway_usage_query', {
      since: null,
      until: null,
      profileId: null,
      limit: null,
    });
  });

  it('forwards gateway_usage_overview without a limit argument', async () => {
    invokeMock.mockResolvedValueOnce({
      requestCount: 2,
      okCount: 1,
      failedCount: 1,
      inputTokens: 16,
      outputTokens: 7,
      cachedInputTokens: 3,
      reasoningTokens: 0,
      avgLatencyMs: 120.5,
      p95LatencyMs: 300,
      avgTtftMs: 40,
    });
    const overview = await gatewayUsageOverview({ until: '2026-08-31T00:00:00+00:00' });

    expect(invokeMock).toHaveBeenCalledWith('gateway_usage_overview', {
      since: null,
      until: '2026-08-31T00:00:00+00:00',
      profileId: null,
    });
    expect(overview).toMatchObject({
      requestCount: 2,
      failedCount: 1,
      p95LatencyMs: 300,
    });
  });
});
