/**
 * Backend product-feature surface for UI gating.
 * Prefer this over offering actions that always throw unsupported in production.
 */
import { getBackend } from '@/app/runtime';
import {
  resolveBackendFeatures,
  type BackendFeatures,
} from '@/lib/backend/contracts';

export type { BackendFeatures };

/** Features implemented by the active backend (fail-closed defaults). */
export function getBackendFeatures(): BackendFeatures {
  return resolveBackendFeatures(getBackend().features);
}
