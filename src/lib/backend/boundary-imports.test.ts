/**
 * Architecture guard: only `lib/backend/tauri/**` may call native shell APIs.
 * Pages/hooks must use façades; production must not import `src/dev/*`.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

function sourceOf(rel: string): string {
  return readFileSync(path.join(srcRoot, rel), 'utf8');
}

function isTestOrDevFile(relPosix: string): boolean {
  if (relPosix.startsWith('dev/') || relPosix.startsWith('test/')) return true;
  if (/\.(test|spec)\.(ts|tsx)$/.test(relPosix)) return true;
  if (relPosix.includes('/__tests__/')) return true;
  return false;
}

function walkSourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
    const full = path.join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      walkSourceFiles(full, out);
      continue;
    }
    if (/\.(ts|tsx)$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

function toPosixRel(abs: string): string {
  return path.relative(srcRoot, abs).split(path.sep).join('/');
}

/** Real imports only (ignore comments / docstrings that mention package names). */
const TAURI_IMPORT_RE =
  /(?:from|import)\s+['"]@tauri-apps\/[^'"]+['"]|import\s*\(\s*['"]@tauri-apps\/[^'"]+['"]\s*\)/;
const BACKEND_TAURI_IMPORT_RE =
  /from\s+['"]@\/lib\/backend\/tauri(?:\/[^'"]*)?['"]|from\s+['"](?:\.\.\/)+backend\/tauri(?:\/[^'"]*)?['"]/;
const DEV_IMPORT_RE =
  /from\s+['"]@\/dev\/|from\s+['"](?:\.\.\/)*dev\/|import\s*\(\s*['"]@\/dev\//;
/** Direct core invoke import (must only live in tauri/invoke.ts). */
const DIRECT_TAURI_CORE_INVOKE_RE =
  /import\s*\{[^}]*\binvoke\b[^}]*\}\s*from\s*['"]@tauri-apps\/api\/core['"]/;

describe('pages/hooks façade patterns (spot checks)', () => {
  it('agent-card uses the install façade, not tauri install-events', () => {
    const src = sourceOf('pages/agents/agent-card.tsx');
    expect(src).not.toMatch(/@\/lib\/backend\/tauri/);
    expect(src).not.toMatch(/isTauriApp/);
    expect(src).toMatch(/from '@\/lib\/api\/install'/);
  });

  it('useSkills uses the skill façade, not tauri skill-events', () => {
    const src = sourceOf('lib/hooks/useSkills.ts');
    expect(src).not.toMatch(/@\/lib\/backend\/tauri/);
    expect(src).not.toMatch(/isTauriApp/);
    expect(src).toMatch(/onSkillsFsChanged/);
    expect(src).toMatch(/from '@\/lib\/api\/skill'/);
  });

  it('tauri update uses the shared invoke wrapper for controlled restart', () => {
    const src = sourceOf('lib/backend/tauri/update.ts');
    expect(src).not.toMatch(/from '@tauri-apps\/api\/core'/);
    expect(src).toMatch(/from '\.\/invoke'/);
    expect(src).toMatch(/invoke\('request_controlled_restart'\)/);
  });
});

describe('production module graph boundary (full src scan)', () => {
  const productionFiles = walkSourceFiles(srcRoot)
    .map(toPosixRel)
    .filter((rel) => !isTestOrDevFile(rel));

  it('only lib/backend/tauri (and platform.ts isTauri) may import @tauri-apps', () => {
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      const src = sourceOf(rel);
      if (!TAURI_IMPORT_RE.test(src)) continue;
      const allowed =
        rel.startsWith('lib/backend/tauri/') ||
        rel === 'lib/platform.ts'; // isTauri() only; must not choose mock transport
      if (!allowed) offenders.push(rel);
    }
    expect(offenders).toEqual([]);
  });

  it('only lib/backend/tauri/invoke.ts may import invoke from @tauri-apps/api/core', () => {
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      const src = sourceOf(rel);
      if (!DIRECT_TAURI_CORE_INVOKE_RE.test(src)) continue;
      if (rel !== 'lib/backend/tauri/invoke.ts') offenders.push(rel);
    }
    expect(offenders).toEqual([]);
  });

  it('pages and hooks never import lib/backend/tauri or src/dev', () => {
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      if (!rel.startsWith('pages/') && !rel.startsWith('lib/hooks/')) continue;
      const src = sourceOf(rel);
      if (BACKEND_TAURI_IMPORT_RE.test(src) || DEV_IMPORT_RE.test(src)) {
        offenders.push(rel);
      }
    }
    expect(offenders).toEqual([]);
  });

  it('production (non-tauri) lib must not import @/lib/backend/tauri', () => {
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      if (!rel.startsWith('lib/')) continue;
      if (rel.startsWith('lib/backend/tauri/')) continue;
      // Tests and shape checks may live under lib/api; production api façades use getBackend only.
      const src = sourceOf(rel);
      if (BACKEND_TAURI_IMPORT_RE.test(src)) offenders.push(rel);
    }
    expect(offenders).toEqual([]);
  });

  it('production code outside dev never imports @/dev/*', () => {
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      const src = sourceOf(rel);
      if (DEV_IMPORT_RE.test(src)) offenders.push(rel);
    }
    expect(offenders).toEqual([]);
  });

  it('lib/backend/tauri never imports @/lib/api (contracts/runtime only)', () => {
    const apiImportRe = /from\s+['"]@\/lib\/api(?:\/[^'"]*)?['"]/;
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      if (!rel.startsWith('lib/backend/tauri/')) continue;
      if (apiImportRe.test(sourceOf(rel))) offenders.push(rel);
    }
    expect(offenders).toEqual([]);
  });

  it('pages do not import applyAdapter (bindTicket is the product write path)', () => {
    const applyAdapterIdent = /(?<![A-Za-z])applyAdapter(?![A-Za-z])/;
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      if (!rel.startsWith('pages/')) continue;
      if (applyAdapterIdent.test(sourceOf(rel))) offenders.push(rel);
    }
    expect(offenders).toEqual([]);
  });

  it('pages/bridges does not import pages/connections, and layout does not import bridges models', () => {
    const bridgesToConnections = /from\s+['"]@\/pages\/connections(?:\/[^'"]*)?['"]/;
    const layoutToBridgesModel = /from\s+['"]@\/pages\/bridges\/[^'"]+['"]/;
    const offenders: string[] = [];
    for (const rel of productionFiles) {
      const src = sourceOf(rel);
      if (rel.startsWith('pages/bridges/') && bridgesToConnections.test(src)) {
        offenders.push(rel);
      }
      if (
        (rel === 'App.tsx' || rel.startsWith('components/layout/'))
        && layoutToBridgesModel.test(src)
      ) {
        offenders.push(rel);
      }
    }
    expect(offenders).toEqual([]);
  });
});
