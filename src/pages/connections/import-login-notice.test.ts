import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import type { ImportLoginReport } from '@/lib/api/account';
import type { Account } from '@/lib/types';
import { importLoginErrorNotice, importLoginReportNotice } from './import-login-notice';

const dir = path.dirname(fileURLToPath(import.meta.url));
const zh = createTranslator('zh');
const en = createTranslator('en');

function report(partial: Partial<ImportLoginReport> = {}): ImportLoginReport {
  return {
    account: { id: 'pi-1', agentId: 'pi', kind: 'oauth', label: 'Anthropic' } as Account,
    importedCount: 1,
    skippedInTrash: [],
    skippedLocalRoute: 0,
    failed: [],
    ...partial,
  };
}

describe('manual import notice', () => {
  it('keeps the usual success toast when nothing else happened', () => {
    const notice = importLoginReportNotice(report(), 'Anthropic 已加入列表', zh);
    expect(notice).toEqual({
      title: zh('connections.import.toastOk'),
      description: 'Anthropic 已加入列表',
      variant: 'success',
    });
  });

  it('names the logins left in the recycle bin in one plain sentence', () => {
    const notice = importLoginReportNotice(
      report({
        importedCount: 1,
        skippedInTrash: [
          { id: 'trash-2', label: 'xAI', home: 'connections' },
          { id: 'trash-3', label: 'OpenAI', home: 'route_pool' },
        ],
      }),
      'Anthropic 已加入列表',
      zh,
    );
    expect(notice).toEqual({
      title: zh('connections.import.toastOk'),
      description: '已导入 1 个登录。另有 2 个在回收站里，没有导入：xAI、OpenAI。',
      variant: 'success',
    });
    expect(Object.keys(notice)).not.toContain('action');
  });

  it('says so when every login is in the recycle bin', () => {
    const notice = importLoginReportNotice(
      report({
        account: null,
        importedCount: 0,
        skippedInTrash: [{ id: 'trash-1', label: 'xAI', home: 'connections' }],
      }),
      '',
      zh,
    );
    expect(notice).toEqual({ title: '这些登录都在回收站里，没有导入', variant: 'warning' });
    expect(en('connections.import.toastAllInTrash')).toBe(
      'These logins are all in Trash and were not imported',
    );
  });

  it('mentions skipped local-route entries and failures only when present', () => {
    const notice = importLoginReportNotice(
      report({
        skippedLocalRoute: 1,
        failed: [{ label: 'broken', code: 'invalid_arg', message: 'bad' }],
      }),
      'Anthropic 已加入列表',
      zh,
    );
    expect(notice.description).toBe(
      'Anthropic 已加入列表 另有 1 个没导入成功：broken。 另有 1 个是本机路由写进去的配置，已跳过。',
    );
    expect(notice.variant).toBe('warning');
    expect(importLoginReportNotice(report(), 'x', zh).description).not.toContain('本机路由');
  });

  it('shows the import error as is', () => {
    const notice = importLoginErrorNotice(new Error('boom [account.import]'), zh);
    expect(notice.title).toBe(zh('connections.import.toastFail'));
    expect(notice.description).toBe('boom [account.import]');
    expect(notice.variant).toBe('danger');
  });

  it('avoids internal words in the new copy', () => {
    const keys = [
      'toastPartialTrash',
      'toastAllInTrash',
      'toastSkippedLocalRoute',
      'toastSomeFailed',
    ] as const;
    for (const key of keys) {
      const text = zh(`connections.import.${key}`);
      expect(text).not.toMatch(/凭据|投影|live|真源|票/);
    }
  });
});

describe('connections page import wiring', () => {
  const page = readFileSync(path.join(dir, 'index.tsx'), 'utf8');

  it('manual import reads the report and shows the notice', () => {
    const manual = page.slice(page.indexOf('const confirmImportLogin'));
    expect(manual).toContain('await importCurrentLoginWithReport(addAgentId)');
    expect(manual).toContain('importLoginReportNotice(report, baseDescription, t)');
    expect(manual).toContain('importLoginErrorNotice(e, t)');
  });

  it('auto import stays quiet when the logins are in the recycle bin', () => {
    const start = page.indexOf('autoImportTriedRef.current.add(agentId)');
    const auto = page.slice(start, page.indexOf('const handleTrashChanged'));
    expect(auto).toContain('await importCurrentLoginWithReport(agentId)');
    expect(auto).toContain('if (!report.account) continue;');
    expect(auto).not.toContain('importLoginReportNotice');
    expect(auto).not.toContain('toastAllInTrash');
  });
});
