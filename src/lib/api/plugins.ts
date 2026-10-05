import { getBackend } from '@/app/runtime';
import type {
  PluginEntry,
  PluginInstallOptions,
  PluginInventory,
  PluginMutationOutcome,
  PluginUninstallOptions,
  PluginUpdateOptions,
} from '@/lib/backend/contracts/plugin-types';
import type { AgentKey } from '@/lib/types';

/** Scan of vendor plugin / extension packs (not MCP). Claude, Codex, Grok, and Pi. */
export async function listPluginInventory(): Promise<PluginInventory> {
  return getBackend().plugins.listInventory();
}

/** Vendor-listed packs that can be installed. Pi has no marketplace list. */
export async function listAvailablePlugins(agent: AgentKey): Promise<PluginEntry[]> {
  return getBackend().plugins.listAvailable(agent);
}

/** Component preview before confirm. Does not install. */
export async function previewPluginInstall(agent: AgentKey, source: string): Promise<PluginEntry> {
  return getBackend().plugins.previewInstall(agent, source);
}

/** Official install after UI confirm. Only Grok maps confirmation to `--trust`. */
export async function installPlugin(
  agent: AgentKey,
  source: string,
  options: PluginInstallOptions,
): Promise<PluginMutationOutcome> {
  return getBackend().plugins.install(agent, source, options);
}

/** Vendor uninstall. Claude/Grok keep their separate plugin data directory by default. */
export async function uninstallPlugin(
  agent: AgentKey,
  name: string,
  marketplace: string | null | undefined,
  installSource: string | null | undefined,
  options: PluginUninstallOptions,
): Promise<PluginMutationOutcome> {
  return getBackend().plugins.uninstall(agent, name, marketplace, installSource, options);
}

/** Turn on a listed Claude, Codex, or Grok pack through that Agent's supported command/config. */
export async function enablePlugin(
  agent: AgentKey,
  name: string,
  marketplace?: string | null,
): Promise<PluginMutationOutcome> {
  return getBackend().plugins.enable(agent, name, marketplace);
}

/** Turn off a listed Claude, Codex, or Grok pack through that Agent's supported command/config. */
export async function disablePlugin(
  agent: AgentKey,
  name: string,
  marketplace?: string | null,
): Promise<PluginMutationOutcome> {
  return getBackend().plugins.disable(agent, name, marketplace);
}

/** Refresh Claude, Codex, or Grok marketplace catalogs. This does not update installed packs. */
export async function refreshPluginMarketplace(agent: AgentKey): Promise<PluginMutationOutcome> {
  return getBackend().plugins.refreshMarketplace(agent);
}

/** Update one installed Claude or Grok plugin pack after confirmation. */
export async function updatePlugin(
  agent: AgentKey,
  name: string,
  marketplace: string | null | undefined,
  scope: string | null | undefined,
  options: PluginUpdateOptions,
): Promise<PluginMutationOutcome> {
  return getBackend().plugins.update(agent, name, marketplace, scope, options);
}

/** Update eligible Pi extensions after confirmation. */
export async function updatePiPlugins(options: PluginUpdateOptions): Promise<PluginMutationOutcome> {
  return getBackend().plugins.updatePi(options);
}
