import { describe, expect, it } from 'vitest';
import { applyIdOrder, mergeLiveMove, moveId, persistIdOrder, subscribeIdOrder } from './list-order';

describe('list order', () => {
  it('keeps the live sequence when nothing is stored', () => {
    expect(applyIdOrder(
      [{ id: 'b' }, { id: 'a' }],
      (row) => row.id,
      [],
    ).map((row) => row.id)).toEqual(['b', 'a']);
  });

  it('applies stored ids and appends newcomers', () => {
    expect(applyIdOrder(
      [{ id: 'c' }, { id: 'a' }, { id: 'b' }],
      (row) => row.id,
      ['b', 'a', 'gone'],
    ).map((row) => row.id)).toEqual(['b', 'a', 'c']);
  });

  it('moves an id among a full list', () => {
    expect(moveId(['a', 'b', 'c'], 'c', 'a')).toEqual(['c', 'a', 'b']);
    expect(moveId(['a', 'b', 'c'], 'a', 'c')).toEqual(['b', 'c', 'a']);
    expect(moveId(['a', 'b', 'c'], 'a', 'a')).toEqual(['a', 'b', 'c']);
  });

  it('reorders only the visible subset and keeps hidden ids in place', () => {
    expect(mergeLiveMove(
      ['a', 'x', 'b', 'y', 'c'],
      ['a', 'b', 'c'],
      'c',
      'a',
    )).toEqual(['c', 'x', 'a', 'y', 'b']);
  });

  it('seeds from the live list when storage is empty', () => {
    expect(mergeLiveMove([], ['a', 'b', 'c'], 'b', 'a')).toEqual(['b', 'a', 'c']);
  });

  it('notifies subscribers when an order is persisted', () => {
    const key = 'agenthub:test-order-subscribe';
    let calls = 0;
    const stop = subscribeIdOrder(key, () => {
      calls += 1;
    });
    persistIdOrder(key, ['b', 'a']);
    stop();
    expect(calls).toBe(1);
  });
});
