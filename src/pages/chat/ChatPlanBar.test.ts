import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { RuntimePlanEntry } from '@/lib/api/chat';
import { TooltipProvider } from '@/components/ui/tooltip';
import { ChatPlanBar } from './ChatPlanBar';

function renderPlan(plan?: RuntimePlanEntry[] | null): string {
  return renderToStaticMarkup(createElement(TooltipProvider, null, createElement(ChatPlanBar, { plan })));
}

describe('ChatPlanBar', () => {
  it('draws nothing when the plan is missing or only blank rows', () => {
    expect(renderPlan(undefined)).toBe('');
    expect(renderPlan(null)).toBe('');
    expect(renderPlan([])).toBe('');
    expect(renderPlan([{ content: '   ' }])).toBe('');
  });

  it('shows progress, live/failed counts, and every kept row when expanded', () => {
    const html = renderPlan([
      { content: 'read', status: 'completed' },
      { content: 'edit', status: 'in_progress' },
      { content: '  ' },
      { content: 'test', status: 'pending' },
      { content: 'broken', status: 'failed' },
    ]);
    expect(html).toContain('data-help="chat-plan-bar"');
    expect(html).toContain('aria-expanded="true"');
    expect(html).toContain('1/4 已完成');
    expect(html).toContain('进行中 1');
    expect(html).toContain('失败 1');
    expect(html).toContain('已完成');
    expect(html).toContain('read');
    expect(html).toContain('edit');
    expect(html).toContain('test');
    expect(html).toContain('broken');
    expect(html).toContain('收起计划');
    expect(html).toContain('data-help="chat-expand-affordance"');
    expect(html).not.toContain('  ');
  });

  it('maps vendor status aliases onto the same live / done / pending / failed labels', () => {
    const html = renderPlan([
      { content: 'done-row', status: 'complete' },
      { content: 'live-row', status: 'running' },
      { content: 'fail-row', status: 'canceled' },
      { content: 'wait-row' },
    ]);
    expect(html).toContain('1/4 已完成');
    expect(html).toContain('进行中 1');
    expect(html).toContain('失败 1');
    expect(html).toContain('待做');
    expect(html).toContain('done-row');
    expect(html).toContain('live-row');
    expect(html).toContain('fail-row');
    expect(html).toContain('wait-row');
  });
});
