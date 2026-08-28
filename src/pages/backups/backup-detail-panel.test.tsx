import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { TooltipProvider } from '@/components/ui/tooltip';
import { BackupDetailPanel } from './backup-detail-panel';

describe('BackupDetailPanel', () => {
  it('shows identity in the details header and never a raw secret', () => {
    const secret = 'sk-ant-secret-12345678';
    const markup = renderToStaticMarkup(
      createElement(TooltipProvider, null, createElement(BackupDetailPanel, {
        backup: {
          id: 'bk-1',
          agentId: 'claude',
          kind: 'manual',
          createdAt: '2026-01-01T00:00:00.000Z',
          files: ['settings.json'],
          sizeBytes: 128,
          identity: 'ada@claude.test',
        },
        kindLabel: '手动',
        busy: false,
        onClose() {},
        onRestore() {},
        onDelete() {},
      })),
    );
    expect(markup).toContain('data-side-inspect');
    expect(markup).toContain('备份详情');
    expect(markup).toContain('ada@claude.test');
    expect(markup).toContain('相关文件');
    expect(markup).toContain('收起');
    expect(markup).not.toContain('取消');
    expect(markup).not.toContain(secret);
  });
});
