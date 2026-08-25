import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { InspectSurface } from './InspectSurface';

vi.mock('@/components/ui/dialog', () => {
  const passthrough = ({ children }: { children?: ReactNode }) => children ?? null;
  const dialogContent = ({
    children,
    onPointerDownOutside,
  }: {
    children?: ReactNode;
    onPointerDownOutside?: unknown;
  }) => createElement(
    'div',
    { 'data-prevent-dismiss': onPointerDownOutside ? 'true' : undefined },
    children,
  );
  return {
    Dialog: ({ open, children }: { open?: boolean; children?: ReactNode }) =>
      (open ? children : null),
    DialogContent: dialogContent,
    DialogHeader: passthrough,
    DialogFooter: passthrough,
    DialogTitle: passthrough,
    DialogDescription: passthrough,
  };
});

describe('InspectSurface', () => {
  it('does not mount a closed right-side panel', () => {
    const markup = renderToStaticMarkup(
      createElement(InspectSurface, {
        asPanel: true,
        open: false,
        onOpenChange: () => undefined,
        title: 'Details',
        children: createElement('p', null, 'Body'),
      }),
    );

    expect(markup).toBe('');
  });

  it('renders panel chrome and keeps actions in the header', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(InspectSurface, {
          asPanel: true,
          open: true,
          onOpenChange: () => undefined,
          title: 'Details',
          description: 'Inspect route',
          primary: createElement('button', null, 'Save'),
          danger: createElement('button', null, 'Delete'),
          children: createElement('p', null, 'Body'),
        }),
      ),
    );

    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('Details');
    expect(markup).toContain('Inspect route');
    expect(markup).toContain('Save');
    expect(markup).toContain('Delete');
    expect(markup).toContain('Body');
    expect(markup).toContain('取消');
  });

  it('omits cancel on read-only inspect panels', () => {
    const markup = renderToStaticMarkup(
      createElement(
        TooltipProvider,
        null,
        createElement(InspectSurface, {
          asPanel: true,
          open: true,
          onOpenChange: () => undefined,
          title: 'Details',
          showCancel: false,
          primary: createElement('button', null, 'Edit'),
          danger: createElement('button', null, 'Delete'),
          children: createElement('p', null, 'Body'),
        }),
      ),
    );

    expect(markup).toContain('Edit');
    expect(markup).toContain('Delete');
    expect(markup).not.toContain('取消');
  });

  it('renders the same content in dialog mode', () => {
    const markup = renderToStaticMarkup(
      createElement(InspectSurface, {
        open: true,
        onOpenChange: () => undefined,
        title: 'Edit route',
        preventDismiss: true,
        primary: createElement('button', null, 'Apply'),
        children: createElement('p', null, 'Form'),
      }),
    );

    expect(markup).toContain('Edit route');
    expect(markup).toContain('Apply');
    expect(markup).toContain('Form');
    expect(markup).toContain('data-prevent-dismiss="true"');
    expect(markup).not.toContain('data-side-inspect');
  });
});
