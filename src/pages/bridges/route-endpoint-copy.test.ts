import { describe, expect, it } from 'vitest';
import {
  formatInboundAt,
  localAddressCopyForTarget,
  ROUTE_LOCAL_ADDRESS_LEGEND,
  routeEndpointCopyKey,
} from './route-endpoint-copy';

describe('route endpoint copy', () => {
  it('explains the three conversation addresses plus GET /models', () => {
    expect(ROUTE_LOCAL_ADDRESS_LEGEND.map((row) => [row.method, row.path, row.copyKey])).toEqual([
      ['POST', '/v1/messages', 'routes.endpoint.messages'],
      ['POST', '/v1/responses', 'routes.endpoint.responses'],
      ['POST', '/v1/chat/completions', 'routes.endpoint.chatCompletions'],
      ['GET', '/models', 'routes.endpoint.models'],
    ]);
    expect(routeEndpointCopyKey('messages')).toBe('routes.endpoint.messages');
    expect(routeEndpointCopyKey('responses')).toBe('routes.endpoint.responses');
    expect(routeEndpointCopyKey('chat_completions')).toBe('routes.endpoint.chatCompletions');
    expect(localAddressCopyForTarget('claude')).toEqual({
      path: '/v1/messages',
      copyKey: 'routes.endpoint.messages',
    });
    expect(localAddressCopyForTarget('codex').path).toBe('/v1/responses');
    expect(localAddressCopyForTarget('grok').path).toBe('/v1/responses');
  });

  it('formats inbound time without depending on the local clock', () => {
    expect(formatInboundAt('2026-08-12T00:00:02.000Z')).toBe('2026-08-12 00:00:02');
    expect(formatInboundAt('')).toBe('');
  });
});
