/** Compatibility re-export. Implementation lives in backend contracts. */
export {
  applyEffectiveConnection,
  enrichStatusesWithConnections,
  extractProviderEndpoint,
  formatApiConnectionLabel,
  formatEndpointHost,
  resolveEffectiveConnection,
  type EffectiveConnection,
} from '@/lib/backend/contracts/agent-connection';
