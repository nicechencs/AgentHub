#!/usr/bin/env node
/**
 * Sync release version from package.json into Cargo.toml and Cargo.lock.
 *
 * package.json is the single source of truth. tauri.conf.json should reference
 * ../package.json so the desktop bundle reads the same version at build time.
 */
import { pathToFileURL } from 'node:url';
import { syncReleaseVersionFromPackageJson } from './release-metadata.mjs';

function parseArgs(argv) {
  const options = { root: undefined, version: undefined };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--root') {
      options.root = argv[++index];
      if (!options.root) throw new Error('--root requires a directory');
    } else if (argument === '--version') {
      options.version = argv[++index];
      if (!options.version) throw new Error('--version requires a value');
    } else if (argument === '--help' || argument === '-h') {
      console.log('Usage: node scripts/release-sync-version.mjs [--root DIR] [--version X.Y.Z]');
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  return options;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const metadata = syncReleaseVersionFromPackageJson(options.root, options.version);
  console.log(JSON.stringify(metadata));
}

const invokedPath = process.argv[1] ? pathToFileURL(process.argv[1]).href : null;
if (invokedPath === import.meta.url) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
