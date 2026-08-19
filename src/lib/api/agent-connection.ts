/** Compatibility re-export. Implementation lives in backend contracts. */
export {
  applyEffectiveConnection,
  enrichStatusesWithConnections,
  extractProviderEndpoint,
  formatApiConnectionLabel,
  formatEndpointHost,
  formatLocalRouteLabel,
  isInternalGeneratedName,
  isInternalGeneratedProvider,
  isLoopbackUrl,
  resolveEffectiveConnection,
  type EffectiveConnection,
  type FormatApiConnectionLabelOptions,
} from '@/lib/backend/contracts/agent-connection';
