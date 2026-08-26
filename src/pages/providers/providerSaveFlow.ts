/**
 * Page barrel: schema UI gate + provider save use case.
 * Catalog/schema gating stays here; save orchestration is `@/lib/api/provider-save`.
 */
export {
  canSaveProviderForm,
  canSaveWithSchemaStatus,
  planSchemaLoad,
  resolveProjectorExpectation,
  schemaErrorMessage,
  type ProjectorExpectation,
  type SchemaLoadPlan,
  type SchemaUiStatus,
} from './provider-schema-gate';

export {
  parseJsonConfigBase,
  projectValuesToSchema,
  resolveSavePath,
  runProviderSaveFlow,
  type ProviderSaveFailureCode,
  type ProviderSaveFlowDeps,
  type ProviderSaveFlowInput,
  type ProviderSavePath,
  type ProviderSaveResult,
} from '@/lib/api/provider-save';
