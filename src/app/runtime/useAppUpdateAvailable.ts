import { useSyncExternalStore } from 'react';
import {
  getAppUpdateAvailable,
  subscribeAppUpdate,
} from './app-update-store';
import type { UpdateInfo } from '@/lib/backend/contracts/update-types';

/** Subscribe to pending AgentHub self-update info (null when none / cleared). */
export function useAppUpdateAvailable(): UpdateInfo | null {
  return useSyncExternalStore(
    subscribeAppUpdate,
    getAppUpdateAvailable,
    getAppUpdateAvailable,
  );
}
