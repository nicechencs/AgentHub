import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import type { GoRouteIsolatedState } from '@/lib/backend/contracts/go-route-isolated';
import { createTranslator } from '@/lib/i18n';
import {
  GO_ISOLATED_CONFIG_UPDATE_ERROR,
  goIsolatedVisibleError,
  retainGoIsolatedConfigError,
} from './go-isolated-strip';

function stripStatus(state: GoRouteIsolatedState, lastError: string | null) {
  return { state, lastError };
}

const zh = createTranslator('zh');
const en = createTranslator('en');

describe('goIsolatedVisibleError', () => {
  it('shows a short config error while the process is still running', () => {
    expect(goIsolatedVisibleError({
      state: 'ready',
      lastError: GO_ISOLATED_CONFIG_UPDATE_ERROR,
    }, zh)).toBe('配置没更新成功');
    expect(goIsolatedVisibleError({
      state: 'ready',
      lastError: `  ${GO_ISOLATED_CONFIG_UPDATE_ERROR}  `,
    }, en)).toBe('Configuration could not be updated');
    expect(goIsolatedVisibleError({
      state: 'starting',
      lastError: GO_ISOLATED_CONFIG_UPDATE_ERROR,
    }, zh)).toBe('配置没更新成功');
  });

  it('keeps the same sentence when the state is failed', () => {
    expect(goIsolatedVisibleError({
      state: 'failed',
      lastError: GO_ISOLATED_CONFIG_UPDATE_ERROR,
    }, zh)).toBe('配置没更新成功');
  });

  it('shows other errors that are not stopped', () => {
    expect(goIsolatedVisibleError({
      state: 'failed',
      lastError: 'Go route could not start',
    }, zh)).toBe('Go route could not start');
  });

  it('adds no line when there is no error or the process is stopped', () => {
    expect(goIsolatedVisibleError({ state: 'ready', lastError: null }, zh)).toBeNull();
    expect(goIsolatedVisibleError({ state: 'ready', lastError: '   ' }, zh)).toBeNull();
    expect(goIsolatedVisibleError({ state: 'failed', lastError: null }, zh)).toBeNull();
    expect(goIsolatedVisibleError({
      state: 'stopped',
      lastError: GO_ISOLATED_CONFIG_UPDATE_ERROR,
    }, zh)).toBeNull();
  });
});

describe('retainGoIsolatedConfigError', () => {
  it('keeps the config error across a running status poll that drops it', () => {
    const next = retainGoIsolatedConfigError(
      stripStatus('ready', GO_ISOLATED_CONFIG_UPDATE_ERROR),
      stripStatus('ready', null),
    );
    expect(next.lastError).toBe(GO_ISOLATED_CONFIG_UPDATE_ERROR);
    expect(next.state).toBe('ready');
  });

  it('drops the line when the process stops or a newer error arrives', () => {
    const current = stripStatus('ready', GO_ISOLATED_CONFIG_UPDATE_ERROR);
    expect(retainGoIsolatedConfigError(current, stripStatus('stopped', null)).lastError).toBeNull();
    expect(retainGoIsolatedConfigError(
      current,
      stripStatus('failed', 'Go route process exited'),
    ).lastError).toBe('Go route process exited');
    expect(retainGoIsolatedConfigError(
      stripStatus('ready', null),
      stripStatus('ready', null),
    ).lastError).toBeNull();
  });
});

describe('Go route strip wiring', () => {
  it('renders the error from lastError and leaves the running label on state', () => {
    const page = readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), 'index.tsx'),
      'utf8',
    );
    expect(page).toContain('goIsolatedVisibleError(status, t)');
    expect(page).toContain('retainGoIsolatedConfigError(current, next)');
    expect(page).toContain('goIsolatedStatusLabel(status.state, t)');
    expect(page).toContain('goIsolatedBadgeVariant(status.state)');
    expect(page).not.toContain("status.state === 'failed'");
  });
});
