#!/usr/bin/env node

import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const defaultRoot = path.resolve(scriptDir, '..');
const manifestSchemaVersion = 1;

const TARGETS = new Map([
  ['x86_64-unknown-linux-gnu', { goos: 'linux', goarch: 'amd64', extension: '' }],
  ['aarch64-unknown-linux-gnu', { goos: 'linux', goarch: 'arm64', extension: '' }],
  ['x86_64-apple-darwin', { goos: 'darwin', goarch: 'amd64', extension: '' }],
  ['aarch64-apple-darwin', { goos: 'darwin', goarch: 'arm64', extension: '' }],
  ['x86_64-pc-windows-msvc', { goos: 'windows', goarch: 'amd64', extension: '.exe' }],
  ['aarch64-pc-windows-msvc', { goos: 'windows', goarch: 'arm64', extension: '.exe' }],
]);

export function targetSpec(target) {
  const spec = TARGETS.get(target);
  if (!spec) throw new Error(`Unsupported desktop sidecar target: ${target}`);
  return spec;
}

export function parseRustHost(stdout) {
  const match = stdout.match(/^host:\s*(\S+)\s*$/m);
  if (!match) throw new Error('rustc -vV did not report a host target');
  return match[1];
}

export function sidecarFileName(target) {
  const { extension } = targetSpec(target);
  return `agenthub-adapterd-${target}${extension}`;
}

export function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

export function readPackageVersion(root) {
  const value = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8')).version;
  if (typeof value !== 'string' || !/^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?$/.test(value)) {
    throw new Error('package.json must contain a valid desktop version');
  }
  return value;
}

export function sidecarPaths(root, target) {
  const directory = path.join(root, 'src-tauri', 'binaries');
  const fileName = sidecarFileName(target);
  return {
    directory,
    binary: path.join(directory, fileName),
    manifest: path.join(directory, `${fileName}.json`),
  };
}

export function verifySidecar(root, target) {
  const spec = targetSpec(target);
  const expectedVersion = readPackageVersion(root);
  const paths = sidecarPaths(root, target);
  const manifest = JSON.parse(fs.readFileSync(paths.manifest, 'utf8'));
  const stat = fs.statSync(paths.binary);
  const expected = {
    schemaVersion: manifestSchemaVersion,
    target,
    goos: spec.goos,
    goarch: spec.goarch,
    packageVersion: expectedVersion,
    fileName: path.basename(paths.binary),
    sha256: sha256File(paths.binary),
    size: stat.size,
  };
  for (const [key, value] of Object.entries(expected)) {
    if (manifest[key] !== value) {
      throw new Error(`Sidecar manifest ${key} mismatch: expected ${value}, got ${manifest[key]}`);
    }
  }
  if (!fs.readFileSync(paths.binary).includes(Buffer.from(expectedVersion, 'utf8'))) {
    throw new Error('Sidecar binary does not contain the injected desktop version');
  }
  return { ...paths, manifest };
}

function commandOutput(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: 'utf8', ...options });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || '').trim();
    throw new Error(`${command} failed with exit ${result.status}${detail ? `: ${detail}` : ''}`);
  }
  return result.stdout;
}

export function resolveTarget(explicitTarget) {
  if (explicitTarget) return explicitTarget;
  if (process.env.TAURI_ENV_TARGET_TRIPLE) return process.env.TAURI_ENV_TARGET_TRIPLE;
  if (process.env.CARGO_BUILD_TARGET) return process.env.CARGO_BUILD_TARGET;
  return parseRustHost(commandOutput('rustc', ['-vV']));
}

export function buildSidecar(root, target) {
  const spec = targetSpec(target);
  const version = readPackageVersion(root);
  const paths = sidecarPaths(root, target);
  fs.mkdirSync(paths.directory, { recursive: true });
  const temporary = path.join(
    paths.directory,
    `.${path.basename(paths.binary)}.${process.pid}.${crypto.randomBytes(6).toString('hex')}.tmp`,
  );
  try {
    commandOutput(
      'go',
      [
        'build',
        '-trimpath',
        '-buildvcs=false',
        '-ldflags',
        `-s -w -X main.packageVersion=${version}`,
        '-o',
        temporary,
        '.',
      ],
      {
        cwd: path.join(root, 'go', 'agenthub-adapterd'),
        env: { ...process.env, CGO_ENABLED: '0', GOOS: spec.goos, GOARCH: spec.goarch },
      },
    );
    fs.rmSync(paths.binary, { force: true });
    fs.renameSync(temporary, paths.binary);
    if (spec.goos !== 'windows') fs.chmodSync(paths.binary, 0o755);
    const stat = fs.statSync(paths.binary);
    const manifest = {
      schemaVersion: manifestSchemaVersion,
      target,
      goos: spec.goos,
      goarch: spec.goarch,
      packageVersion: version,
      fileName: path.basename(paths.binary),
      sha256: sha256File(paths.binary),
      size: stat.size,
    };
    const manifestTemp = `${paths.manifest}.${process.pid}.tmp`;
    fs.writeFileSync(manifestTemp, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 });
    fs.rmSync(paths.manifest, { force: true });
    fs.renameSync(manifestTemp, paths.manifest);
    return verifySidecar(root, target);
  } finally {
    fs.rmSync(temporary, { force: true });
  }
}

export function parseArgs(argv) {
  const parsed = { check: false, root: defaultRoot, target: null };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === '--check') parsed.check = true;
    else if (arg === '--root' || arg === '--target') {
      const value = argv[index + 1];
      if (!value) throw new Error(`${arg} requires a value`);
      parsed[arg.slice(2)] = value;
      index += 1;
    } else throw new Error(`Unknown argument: ${arg}`);
  }
  parsed.root = path.resolve(parsed.root);
  return parsed;
}

function main(argv) {
  const args = parseArgs(argv);
  const target = resolveTarget(args.target);
  const result = args.check ? verifySidecar(args.root, target) : buildSidecar(args.root, target);
  process.stdout.write(`${args.check ? 'verified' : 'built'} ${result.binary}\n`);
  process.stdout.write(`sha256 ${result.manifest.sha256}\n`);
}

if (import.meta.url === pathToFileURL(process.argv[1] || '').href) {
  try {
    main(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}${os.EOL}`);
    process.exitCode = 1;
  }
}
