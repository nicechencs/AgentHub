import { describe, expect, it } from 'vitest';
import {
  ROUTE_ENDPOINTS,
  formatRouteEndpointHttpUrl,
  routeEndpointBrandAgentId,
  routeEndpointHttpParts,
  routeEndpointIdForBinding,
  routeEndpointIdForRuleId,
  routeEndpointIdForTargetAgent,
  routeEndpointPath,
  routeEndpointPathForBinding,
  routeEndpointSurfaceLabel,
} from './route-endpoints';

describe('unified route endpoints', () => {
  it('exposes exactly the three downstream conversation paths', () => {
    expect(ROUTE_ENDPOINTS.map((item) => item.path)).toEqual([
      '/v1/messages',
      '/v1/responses',
      '/v1/chat/completions',
    ]);
  });

  it('maps writer agents onto the surface they consume', () => {
    expect(routeEndpointIdForTargetAgent('claude')).toBe('messages');
    expect(routeEndpointIdForTargetAgent('codex')).toBe('responses');
    expect(routeEndpointIdForTargetAgent('grok')).toBe('responses');
    expect(routeEndpointIdForTargetAgent('kimi')).toBe('chat_completions');
    expect(routeEndpointIdForTargetAgent('dsh')).toBe('chat_completions');
    expect(routeEndpointIdForTargetAgent('pi')).toBe('chat_completions');
  });

  it('prefers rule id over the writer agent when both are present', () => {
    expect(routeEndpointIdForRuleId('grok-subscription-to-claude-v1')).toBe('messages');
    expect(routeEndpointIdForRuleId('codex-subscription-to-claude-responses-v1')).toBe('messages');
    expect(routeEndpointIdForRuleId('kimi-membership-to-codex-v1')).toBe('responses');
    expect(routeEndpointIdForRuleId('codex-subscription-to-grok-v1')).toBe('responses');
    expect(routeEndpointIdForRuleId('codex-subscription-to-kimi-v1')).toBe('chat_completions');
    expect(routeEndpointIdForRuleId('codex-subscription-to-dsh-v1')).toBe('chat_completions');
    expect(routeEndpointIdForRuleId('native-endpoint-to-pi-v1')).toBeNull();
    expect(routeEndpointIdForBinding({
      agentId: 'kimi',
      ruleId: 'kimi-membership-to-codex-v1',
    })).toBe('responses');
    expect(routeEndpointPathForBinding({ agentId: 'claude' })).toBe('/v1/messages');
    expect(routeEndpointPath('chat_completions')).toBe('/v1/chat/completions');
    expect(routeEndpointSurfaceLabel('messages')).toBe('Claude');
    expect(routeEndpointSurfaceLabel('responses')).toBe('Codex');
    expect(routeEndpointSurfaceLabel('chat_completions')).toBe('Kimi');
  });

  it('builds a full loopback HTTP URL and keeps a pending-port placeholder', () => {
    expect(formatRouteEndpointHttpUrl({ path: '/v1/messages', port: 43121 }))
      .toBe('http://127.0.0.1:43121/v1/messages');
    expect(formatRouteEndpointHttpUrl({ path: '/v1/responses' }))
      .toBe('http://127.0.0.1:{port}/v1/responses');
    const parts = routeEndpointHttpParts({ path: '/v1/chat/completions', port: 8123 });
    expect(parts.href).toBe('http://127.0.0.1:8123/v1/chat/completions');
    expect(parts.brandAgentId).toBe('codex');
    expect(routeEndpointBrandAgentId('messages')).toBe('claude');
    expect(routeEndpointBrandAgentId('responses')).toBe('codex');
    expect(routeEndpointBrandAgentId('chat_completions')).toBe('codex');
    expect(routeEndpointHttpParts({ path: '/v1/messages' }).href).toBeNull();
  });
});
