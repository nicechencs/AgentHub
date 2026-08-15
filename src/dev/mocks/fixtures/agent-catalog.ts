/**
 * Mock agent catalog — same shape as Core AgentDescriptor wire format.
 * Product set for dev:mock / vitest; not a production source.
 *
 * To verify open AgentKey: append an entry (e.g. unknown-demo) here only —
 * generic lists pick it up via applyAgentCatalog without page changes.
 */
import type { AgentCatalogEntryDto } from '@/lib/backend/contracts/agent-catalog-types';
import type { AgentCapabilities } from '@/lib/capability';
import { MOCK_CAPABILITIES } from '../capabilities';
import { MOCK_INSTALL_CATALOG } from './install-catalog';

const DISPLAY_NAMES: Record<string, string> = {
  claude: 'Claude Code',
  codex: 'Codex',
  kimi: 'Kimi',
  grok: 'Grok',
  pi: 'Pi',
  workbuddy: 'WorkBuddy',
  cursor: 'Cursor Agent',
  dsh: 'DeepSeek Harness',
};

function capsToDto(
  caps: AgentCapabilities | undefined,
): AgentCatalogEntryDto['capabilities'] {
  const out: AgentCatalogEntryDto['capabilities'] = {};
  if (!caps) return out;
  for (const [k, v] of Object.entries(caps)) {
    if (!v) continue;
    out[k] = {
      level: v.level,
      reason: v.reason ?? null,
      minVersion: v.minVersion ?? null,
    };
  }
  return out;
}

/**
 * Agents with a Config Projector (must match `src/dev/mocks/config.ts` SCHEMAS
 * and core `builtin_config_registry`).
 */
const PROJECTOR_SCHEMA_VERSION: Record<string, number> = {
  claude: 1,
  codex: 1,
  kimi: 1,
  grok: 2,
  dsh: 1,
};

/** Agents without projector: explicit null (legacy applyFormVars path). */
const NO_PROJECTOR = new Set(['cursor', 'pi', 'workbuddy']);

function configSchemaVersionFor(agentId: string): number | null {
  if (agentId in PROJECTOR_SCHEMA_VERSION) {
    return PROJECTOR_SCHEMA_VERSION[agentId]!;
  }
  if (NO_PROJECTOR.has(agentId)) return null;
  // Unknown keys in mock: null (no projector) unless tests override.
  return null;
}

function buildBuiltinCatalog(): AgentCatalogEntryDto[] {
  return MOCK_INSTALL_CATALOG.map((row) => ({
    key: row.agentId,
    displayName: DISPLAY_NAMES[row.agentId] ?? row.agentId,
    integrationVersion: 1,
    capabilities: capsToDto(MOCK_CAPABILITIES[row.agentId]),
    installChannels: row.channels.map((ch) => ({
      id: ch.id,
      label: ch.label,
      command: ch.command,
      requires: ch.requires,
    })),
    configSchemaVersion: configSchemaVersionFor(row.agentId),
  }));
}

/** Default mock catalog (eight built-ins). */
export const MOCK_AGENT_CATALOG: AgentCatalogEntryDto[] = buildBuiltinCatalog();

/** Extra demo agent used in tests — fallback letter/color only. */
export const MOCK_UNKNOWN_DEMO_ENTRY: AgentCatalogEntryDto = {
  key: 'unknown-demo',
  displayName: 'Unknown Demo',
  integrationVersion: 1,
  capabilities: {
    skills: { level: 'unsupported', reason: 'demo unsupported' },
    usage: { level: 'planned', reason: 'demo planned' },
    configWrite: { level: 'unsupported', reason: 'demo' },
    accountSwitch: { level: 'unsupported', reason: 'demo' },
    apiKeyAccount: { level: 'unsupported', reason: 'demo' },
    liveBackup: { level: 'unsupported', reason: 'demo' },
    structuredStream: { level: 'unsupported', reason: 'demo' },
    dangerousMode: { level: 'unsupported', reason: 'demo' },
    projectHistory: { level: 'unsupported', reason: 'demo' },
    projectDelete: { level: 'unsupported', reason: 'demo' },
    providerPresets: { level: 'unsupported', reason: 'demo' },
    mcp: { level: 'planned', reason: 'demo' },
    modelSelect: { level: 'planned', reason: 'demo' },
    sessionResume: { level: 'planned', reason: 'demo' },
  },
  installChannels: [
    {
      id: 'npm',
      label: 'npm demo',
      command: 'npm i -g unknown-demo-cli',
      requires: ['nodejs', 'npm'],
    },
  ],
  configSchemaVersion: null,
};

/** Catalog including unknown-demo (tests / optional demo seed). */
export function mockCatalogWithUnknownDemo(): AgentCatalogEntryDto[] {
  return [...MOCK_AGENT_CATALOG, MOCK_UNKNOWN_DEMO_ENTRY];
}
