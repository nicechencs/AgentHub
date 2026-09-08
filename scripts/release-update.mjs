#!/usr/bin/env node
/**
 * Local signed-artifact helper. Version bumps use `pnpm release:bump`.
 * Publishing stays in GitHub Actions.
 */
import { spawnSync } from 'node:child_process';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const powershell = process.platform === 'win32' ? 'powershell.exe' : 'pwsh';
const script = fileURLToPath(new URL('./release-update.ps1', import.meta.url));

const probe = spawnSync(powershell, ['-NoProfile', '-Command', 'exit 0'], {
  stdio: 'ignore',
});
if (probe.error || probe.status !== 0) {
  console.error(
    'release:update needs PowerShell. On macOS/Linux, use pnpm release:bump to change the version; GitHub Actions publishes the installers.',
  );
  process.exit(1);
}

const result = spawnSync(
  powershell,
  ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', script, ...process.argv.slice(2)],
  { stdio: 'inherit' },
);
process.exit(result.status ?? 1);
