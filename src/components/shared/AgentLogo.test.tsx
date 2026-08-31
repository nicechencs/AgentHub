import * as React from 'react';
import { createElement, type ReactElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { AGENT_DISPLAY } from '@/config/agents';
import { TooltipProvider } from '@/components/ui/tooltip';
import { AgentLogo } from './AgentLogo';

const SVG_AGENT_IDS = [
  'claude',
  'codex',
  'kimi',
  'grok',
  'pi',
  'cursor',
  'dsh',
] as const;

function markup(agentId: string, size: 'sm' | 'md' | 'lg' = 'md'): string {
  return renderToStaticMarkup(
    createElement(
      TooltipProvider,
      null,
      createElement(AgentLogo, { agentId, size }),
    ),
  );
}

/**
 * The project intentionally runs Vitest in Node (without jsdom). This small
 * hook dispatcher lets the component be exercised as a React element tree so
 * the image's real onError handler can be fired and the component re-rendered.
 */
function logoHarness(initialAgentId: string) {
  type LogoState = { src?: string; failed: boolean };
  let state: LogoState | undefined;
  let initialized = false;

  const dispatcher = {
    useState<T>(initialValue: T): [T, (next: T | ((previous: T) => T)) => void] {
      if (!initialized) {
        state = initialValue as LogoState;
        initialized = true;
      }
      return [
        state as T,
        (next) => {
          state = typeof next === 'function'
            ? (next as (previous: T) => T)(state as T) as LogoState
            : next as LogoState;
        },
      ];
    },
  };

  const internals = (
    React as unknown as {
      __SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED: {
        ReactCurrentDispatcher: { current: typeof dispatcher | null };
      };
    }
  ).__SECRET_INTERNALS_DO_NOT_USE_OR_YOU_WILL_BE_FIRED;

  function render(agentId = initialAgentId): ReactElement {
    const previousDispatcher = internals.ReactCurrentDispatcher.current;
    internals.ReactCurrentDispatcher.current = dispatcher;
    try {
      return AgentLogo({ agentId });
    } finally {
      internals.ReactCurrentDispatcher.current = previousDispatcher;
    }
  }

  return { render };
}

function logoCircle(tree: ReactElement): ReactElement {
  return tree.props.children as ReactElement;
}

function logoImage(tree: ReactElement): ReactElement | null {
  const child = logoCircle(tree).props.children;
  return React.isValidElement(child) && child.type === 'img' ? child : null;
}

describe('AgentLogo', () => {
  it('uses a local SVG logo first for every known agent id', () => {
    for (const agentId of SVG_AGENT_IDS) {
      expect(AGENT_DISPLAY[agentId]?.logoSvgSrc, agentId).toBeTruthy();
      expect(AGENT_DISPLAY[agentId]?.logoSrc, agentId).toBeTruthy();

      const html = markup(agentId);
      expect(html, agentId).toContain('<img');
      expect(html, agentId).toContain('alt=""');
      expect(html, agentId).toContain('aria-hidden="true"');
      expect(html, agentId).toContain('.svg');
      expect(html, agentId).not.toContain('.png');
    }
  });

  it.each(['workbuddy', 'zcode'] as const)(
    'uses %s PNG directly because its SVG embeds a raster image',
    (agentId) => {
      expect(AGENT_DISPLAY[agentId].logoSvgSrc).toBeUndefined();
      expect(AGENT_DISPLAY[agentId].logoSrc).toMatch(new RegExp(`${agentId}\\.png$`));

      const html = markup(agentId);
      expect(html).toContain('.png');
      expect(html).not.toContain('.svg');
    },
  );

  it('uses contrasting logo backgrounds without changing letter fallback styling', () => {
    const codexCircle = logoCircle(logoHarness('codex').render());
    expect(codexCircle.props.style).toMatchObject({ backgroundColor: '#111827' });

    const kimiCircle = logoCircle(logoHarness('kimi').render());
    expect(kimiCircle.props.style).toMatchObject({ backgroundColor: '#7c6cff' });

    const unknownCircle = logoCircle(logoHarness('unknown-agent').render());
    expect(unknownCircle.props.style).toMatchObject({ backgroundColor: 'var(--text-muted)' });
  });

  it('keeps the circular initial fallback for an unknown agent', () => {
    const html = markup('unknown-agent');

    expect(html).toContain('aria-label="unknown-agent"');
    expect(html).toContain('rounded-full');
    expect(html).toContain('>U</span>');
    expect(html).not.toContain('<img');
  });

  it('falls back from SVG to PNG and then to the initial after both errors', () => {
    const harness = logoHarness('claude');
    const initial = harness.render();
    const initialImage = logoImage(initial);

    expect(initialImage).not.toBeNull();
    expect(initialImage?.props.src).toMatch(/claude\.svg$/);
    initialImage?.props.onError();

    const pngFallback = harness.render();
    const pngImage = logoImage(pngFallback);
    expect(pngImage).not.toBeNull();
    expect(pngImage?.props.src).toMatch(/claude\.png$/);
    pngImage?.props.onError();

    const failed = harness.render();
    expect(logoImage(failed)).toBeNull();
    expect(logoCircle(failed).props.children).toBe('C');
  });

  it('resets source failure state when switching agents', () => {
    const harness = logoHarness('claude');
    const initial = harness.render();
    logoImage(initial)?.props.onError();
    expect(logoImage(harness.render())?.props.src).toMatch(/claude\.png$/);

    const switched = harness.render('codex');
    const switchedImage = logoImage(switched);
    expect(switchedImage).not.toBeNull();
    expect(switchedImage?.props.src).toMatch(/codex\.svg$/);
    switchedImage?.props.onError();
    const switchedPng = logoImage(harness.render('codex'));
    expect(switchedPng?.props.src).toMatch(/codex\.png$/);
  });

  it('maps the dsh agent to the DeepSeek logo asset', () => {
    expect(AGENT_DISPLAY.dsh.logoSvgSrc).toMatch(/deepseek\.svg$/);
    expect(AGENT_DISPLAY.dsh.logoSrc).toMatch(/deepseek\.png$/);
  });

  it('keeps the hint, accessible name, and size API intact', () => {
    const html = markup('claude', 'lg');

    expect(html).toContain('aria-label="Claude Code"');
    expect(html).toContain('h-10 w-10');
    expect(html).toContain('data-state="closed"');
  });
});
