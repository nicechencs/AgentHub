import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  ticketAddActionDescription,
  ticketAddActionLabel,
  ticketAddImportHighlighted,
} from './ticket-add-menu';

describe('ticket add menu copy', () => {
  const t = createTranslator('zh');

  it('names import separately from official login and gives each item one line', () => {
    expect(t('connections.list.add')).toBe('添加登录');
    expect(ticketAddActionLabel('import-login', t)).toBe('导入本机登录');
    expect(ticketAddActionLabel('oauth', t)).toBe('官方登录');
    for (const kind of ['import-login', 'oauth', 'api-key'] as const) {
      expect(ticketAddActionDescription(kind, t)).not.toBe('');
    }
  });

  it('highlights import only for the Agent discovery found', () => {
    expect(ticketAddImportHighlighted('import-login', 'claude', 'claude')).toBe(true);
    expect(ticketAddImportHighlighted('import-login', 'codex', 'claude')).toBe(false);
    expect(ticketAddImportHighlighted('oauth', 'claude', 'claude')).toBe(false);
    expect(ticketAddImportHighlighted('import-login', 'claude', null)).toBe(false);
  });
});
