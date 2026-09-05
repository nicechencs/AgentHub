import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { EMPTY_ONBOARDING_USAGE } from './onboarding-model';
import { OnboardingUsageStep } from './OnboardingUsageStep';

function render(selection = EMPTY_ONBOARDING_USAGE): string {
  return renderToStaticMarkup(
    createElement(OnboardingUsageStep, { selection, onToggle: () => {} }),
  );
}

describe('OnboardingUsageStep', () => {
  it('offers local routing and Sub2API as independent choices', () => {
    const markup = render();
    expect(markup).toContain('data-onboarding-step="usage"');
    expect(markup).toContain('data-onboarding-choice="routes"');
    expect(markup).toContain('data-onboarding-choice="sub2api"');
    expect(markup).toContain('本地路由');
    expect(markup).toContain('Sub2API 站点');
    expect(markup).toContain('可在设置里重新打开');
  });

  it('marks selected choices as checked', () => {
    const none = render();
    expect(none.match(/aria-checked="false"/g)?.length).toBe(2);
    expect(none).not.toContain('aria-checked="true"');

    const routes = render({ routes: true, sub2api: false });
    expect(routes.match(/aria-checked="true"/g)?.length).toBe(1);
    expect(routes.match(/aria-checked="false"/g)?.length).toBe(1);
  });
});
