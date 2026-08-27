import type { Backend } from '@/lib/backend/contracts';
import { createBackend } from '@/lib/backend/current';
import { resetRuntimeContext } from './runtime-context';

let instance: Backend | null = null;

export function getBackend(): Backend {
  if (!instance) instance = createBackend();
  return instance;
}

/** Tests / advanced: replace backend instance, then reset shared stores. */
export function setBackend(backend: Backend): void {
  instance = backend;
  resetRuntimeContext();
}

export function resetBackend(): void {
  instance = null;
  resetRuntimeContext();
}
