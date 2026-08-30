import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import {
  RouteEndpointTypeText,
  RouteEndpointUrl,
  routeEndpointTypeColor,
} from './RouteEndpointUrl';

describe('RouteEndpointUrl', () => {
  it('renders the full HTTP origin plus an agent-colored path', () => {
    const html = renderToStaticMarkup(createElement(RouteEndpointUrl, {
      path: '/v1/messages',
      port: 43121,
      endpointId: 'messages',
    }));
    expect(html).toContain('http://127.0.0.1:43121');
    expect(html).toContain('/v1/messages');
    expect(html).toContain('var(--agent-claude)');
    expect(html).not.toContain('>http://127.0.0.1:43121/v1/messages<');
  });

  it('colors Responses and Chat Completions with the Codex token', () => {
    const responses = renderToStaticMarkup(createElement(RouteEndpointUrl, {
      path: '/v1/responses',
      port: 1,
      endpointId: 'responses',
    }));
    const chat = renderToStaticMarkup(createElement(RouteEndpointUrl, {
      path: '/v1/chat/completions',
      endpointId: 'chat_completions',
    }));
    expect(responses).toContain('var(--agent-codex)');
    expect(chat).toContain('http://127.0.0.1:{port}');
    expect(chat).toContain('var(--agent-codex)');
  });

  it('colors endpoint-type copy with the same brand Agent as the path', () => {
    expect(routeEndpointTypeColor('messages')).toBe('var(--agent-claude)');
    expect(routeEndpointTypeColor('responses')).toBe('var(--agent-codex)');
    expect(routeEndpointTypeColor('chat_completions')).toBe('var(--agent-codex)');
    const html = renderToStaticMarkup(
      createElement(RouteEndpointTypeText, {
        endpointId: 'messages',
        children: 'Claude 对话',
      }),
    );
    expect(html).toContain('Claude 对话');
    expect(html).toContain('var(--agent-claude)');
  });
});
