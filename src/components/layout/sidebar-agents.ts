/** Catalog rows that detect currently reports as installed and not hidden. */
export function installedCatalogAgents<T extends { id: string }>(
  catalog: readonly T[],
  statuses: readonly { agentId: string; installed?: boolean; hidden?: boolean }[],
): T[] {
  return catalog.filter((meta) =>
    statuses.some(
      (status) => status.agentId === meta.id && status.installed && !status.hidden,
    ),
  );
}
