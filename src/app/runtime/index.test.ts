import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import * as runtime from './index';

const runtimeDir = path.dirname(fileURLToPath(import.meta.url));

describe('runtime production barrel', () => {
  it('does not re-export per-store reset helpers', () => {
    const src = readFileSync(path.join(runtimeDir, 'index.ts'), 'utf8');
    expect(src).not.toMatch(
      /reset(?:AgentCatalog|AgentStatus|ConnectionPool|TicketWallet|AppUpdate)Store/,
    );
    expect(runtime).not.toHaveProperty('resetAgentCatalogStore');
    expect(runtime).not.toHaveProperty('resetAgentStatusStore');
    expect(runtime).not.toHaveProperty('resetConnectionPoolStore');
    expect(runtime).not.toHaveProperty('resetTicketWalletStore');
    expect(runtime).not.toHaveProperty('resetAppUpdateStore');
  });

  it('keeps backend lifecycle and domain-level store APIs', () => {
    expect(runtime).toHaveProperty('getBackend');
    expect(runtime).toHaveProperty('setBackend');
    expect(runtime).toHaveProperty('resetBackend');
    expect(runtime).toHaveProperty('seedAgentCatalog');
    expect(runtime).toHaveProperty('refreshRuntimeReadModels');
    expect(runtime).toHaveProperty('notifyConnectionPoolChanged');
    expect(runtime).toHaveProperty('notifyTicketWalletChanged');
    expect(runtime).toHaveProperty('beginConnectionPoolMutation');
    expect(runtime).toHaveProperty('endConnectionPoolMutation');
    expect(runtime).toHaveProperty('markConnectionCurrent');
    expect(runtime).toHaveProperty('applyAgentHidden');
    expect(runtime).toHaveProperty('revertAgentHidden');
  });
});
