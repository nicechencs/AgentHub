/** Catalog rows that detect currently reports as installed. */
export function installedCatalogAgents<T extends { id: string }>(
  catalog: readonly T[],
  statuses: readonly { agentId: string; installed?: boolean }[],
): T[] {
  return catalog.filter((meta) =>
    statuses.some((status) => status.agentId === meta.id && status.installed),
  );
}
