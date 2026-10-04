import { getBackend } from '@/app/runtime';
import type {
  PluginEntry,
  PluginInstallOptions,
  PluginInventory,
  PluginUninstallOptions,
  PluginUpdateOptions,
} from '@/lib/backend/contracts/plugin-types';
import type { AgentKey } from '@/lib/types';

/** Scan of vendor plugin / extension packs (not MCP). Claude, Grok, and Pi. */
export async function listPluginInventory(): Promise<PluginInventory> {
  return getBackend().plugins.listInventory();
}

/** Marketplace packs that can be installed. Not the installed inventory. */
export async function listAvailablePlugins(agent: AgentKey): Promise<PluginEntry[]> {
  return getBackend().plugins.listAvailable(agent);
}

/** Component preview before confirm. Does not install. */
export async function previewPluginInstall(agent: AgentKey, source: string): Promise<PluginEntry> {
  return getBackend().plugins.previewInstall(agent, source);
}

/** Official install after UI confirm. Grok `--trust` / Claude `-y`. */
export async function installPlugin(
  agent: AgentKey,
  source: string,
  options: PluginInstallOptions,
): Promise<void> {
  return getBackend().plugins.install(agent, source, options);
}

/** Official uninstall. Default keeps the plugin data directory. */
export async function uninstallPlugin(
  agent: AgentKey,
  name: string,
  marketplace: string | null | undefined,
  options: PluginUninstallOptions,
): Promise<void> {
  return getBackend().plugins.uninstall(agent, name, marketplace, options);
}

/** Turn on a listed Claude or Grok pack via the official command. */
export async function enablePlugin(
  agent: AgentKey,
  name: string,
  marketplace?: string | null,
): Promise<void> {
  return getBackend().plugins.enable(agent, name, marketplace);
}

/** Turn off a listed Claude or Grok pack via the official command. */
export async function disablePlugin(
  agent: AgentKey,
  name: string,
  marketplace?: string | null,
): Promise<void> {
  return getBackend().plugins.disable(agent, name, marketplace);
}

/** Refresh Claude or Grok marketplace catalogs. This does not update installed packs. */
export async function refreshPluginMarketplace(agent: AgentKey): Promise<void> {
  return getBackend().plugins.refreshMarketplace(agent);
}

/** Update one installed Claude or Grok plugin pack after confirmation. */
export async function updatePlugin(
  agent: AgentKey,
  name: string,
  marketplace: string | null | undefined,
  scope: string | null | undefined,
  options: PluginUpdateOptions,
): Promise<void> {
  return getBackend().plugins.update(agent, name, marketplace, scope, options);
}

/** Update eligible Pi extensions after confirmation. */
export async function updatePiPlugins(options: PluginUpdateOptions): Promise<void> {
  return getBackend().plugins.updatePi(options);
}
