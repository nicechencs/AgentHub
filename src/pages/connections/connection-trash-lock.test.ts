import { beforeEach, describe, expect, it } from 'vitest';
import {
  claimConnectionTrashBusy,
  getConnectionTrashBusyIds,
  releaseConnectionTrashBusy,
  resetConnectionTrashBusy,
} from './connection-trash-lock';

describe('connection-trash-lock', () => {
  beforeEach(() => resetConnectionTrashBusy());

  it('serializes mutations across recycle-bin instances', () => {
    expect(claimConnectionTrashBusy('trash-1')).toBe(true);
    expect(claimConnectionTrashBusy('trash-2')).toBe(false);
    expect(claimConnectionTrashBusy('trash-1')).toBe(false);
    expect([...getConnectionTrashBusyIds()]).toEqual(['trash-1']);

    releaseConnectionTrashBusy('trash-2');
    expect([...getConnectionTrashBusyIds()]).toEqual(['trash-1']);

    releaseConnectionTrashBusy('trash-1');
    expect(getConnectionTrashBusyIds().size).toBe(0);
    expect(claimConnectionTrashBusy('trash-2')).toBe(true);
  });
});
