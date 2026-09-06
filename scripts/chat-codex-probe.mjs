#!/usr/bin/env node

/**
 * Probe the newline-delimited JSON protocol exposed by `codex app-server`.
 *
 * This probe deliberately stops after read-only protocol checks. It never
 * starts a turn, approves a server request, submits a user answer, or
 * attempts session recovery.
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const VERIFIED_PROTOCOL_ONLY = 'verified protocol only';
export const DEFAULT_EXECUTABLE = 'codex';
export const DEFAULT_EXECUTABLE_ARGS = Object.freeze(['app-server']);
export const DEFAULT_REQUEST_TIMEOUT_MS = 8_000;
export const DEFAULT_STDERR_LIMIT_BYTES = 64 * 1024;
export const DEFAULT_STDOUT_LINE_LIMIT_BYTES = 1024 * 1024;

// These method names and their request shapes are present in the checked-in
// app-server v2 schema generated from the local Codex CLI 0.153.0 install.
export const PROBE_METHODS = Object.freeze([
  'model/list',
  'skills/list',
  'plugin/installed',
  'thread/start',
]);

export const UNVERIFIED_CAPABILITIES = Object.freeze([
  'model execution',
  'turn streaming',
  'approvals',
  'user input',
  'steering',
  'interrupt/stop',
  'session recovery',
  'file changes',
]);

const INITIALIZE_METHOD = 'initialize';
const INITIALIZED_NOTIFICATION = 'initialized';
const UNKNOWN = 'unknown';

const THREAD_REQUIRED_FIELDS = [
  'cliVersion',
  'createdAt',
  'cwd',
  'ephemeral',
  'id',
  'modelProvider',
  'preview',
  'projectId',
  'sessionId',
  'source',
  'status',
  'turns',
  'updatedAt',
];

function probeError(code) {
  const error = new Error(code);
  error.code = code;
  return error;
}

function isObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function asPositiveInteger(value, fallback) {
  if (value === undefined) return fallback;
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number <= 0) {
    throw new Error('numeric option must be a positive integer');
  }
  return number;
}

function extractVersion(userAgent) {
  const match = String(userAgent ?? '').match(/(?<![0-9A-Za-z])(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?(?:\+[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?(?![0-9A-Za-z.-])/);
  return match ? match[0] : UNKNOWN;
}

function safePlatform(value) {
  const candidate = String(value ?? '').toLowerCase();
  return /^[a-z0-9][a-z0-9._-]{0,31}$/.test(candidate) ? candidate : UNKNOWN;
}

function safeErrorCode(value) {
  return Number.isSafeInteger(value) ? value : UNKNOWN;
}

function methodFailure(reason, extra = {}) {
  return { status: 'failed', reason, ...extra };
}

function methodUnknown(reason) {
  return { status: UNKNOWN, reason };
}

function validateMethodResult(method, result) {
  if (!isObject(result)) return { valid: false, reason: 'invalid_result' };

  if (method === INITIALIZE_METHOD) {
    const required = ['codexHome', 'platformFamily', 'platformOs', 'userAgent'];
    if (!required.every((field) => typeof result[field] === 'string')) {
      return { valid: false, reason: 'invalid_result' };
    }
    return {
      valid: true,
      cliVersion: extractVersion(result.userAgent),
      platform: safePlatform(result.platformOs),
    };
  }

  if (method === 'model/list' || method === 'skills/list') {
    if (!Array.isArray(result.data)) return { valid: false, reason: 'invalid_result' };
    return { valid: true, itemCount: result.data.length };
  }

  if (method === 'plugin/installed') {
    if (!Array.isArray(result.marketplaces)) return { valid: false, reason: 'invalid_result' };
    return { valid: true, itemCount: result.marketplaces.length };
  }

  if (method === 'thread/start') {
    const required = [
      'approvalPolicy',
      'approvalsReviewer',
      'cwd',
      'model',
      'modelProvider',
      'sandbox',
      'thread',
    ];
    if (!required.every((field) => Object.hasOwn(result, field)) || !isObject(result.thread)) {
      return { valid: false, reason: 'invalid_result' };
    }
    if (!THREAD_REQUIRED_FIELDS.every((field) => Object.hasOwn(result.thread, field))) {
      return { valid: false, reason: 'invalid_result' };
    }
    return { valid: true };
  }

  return { valid: false, reason: 'unsupported_probe_method' };
}

function makeIsolatedEnvironment({ codexHome, workspace, tempDirectory }) {
  const isolatedTmp = path.join(tempDirectory, 'tmp');
  const configHome = path.join(codexHome, 'config');
  const dataHome = path.join(codexHome, 'data');

  // Do not spread process.env into the child. In particular, API keys and
  // CODEX_* settings from the user's shell must never reach this probe.
  const environment = {};
  if (process.env.PATH) environment.PATH = process.env.PATH;
  if (process.platform === 'win32') {
    for (const name of ['SystemRoot', 'WINDIR', 'ComSpec', 'PATHEXT', 'SystemDrive']) {
      if (process.env[name]) environment[name] = process.env[name];
    }
  }

  environment.CODEX_HOME = codexHome;
  environment.HOME = codexHome;
  environment.XDG_CONFIG_HOME = configHome;
  environment.XDG_DATA_HOME = dataHome;
  environment.TMPDIR = isolatedTmp;
  environment.TMP = isolatedTmp;
  environment.TEMP = isolatedTmp;
  if (process.platform === 'win32') environment.USERPROFILE = codexHome;

  // The value is intentionally unused in requests; it keeps the child cwd
  // explicit and makes accidental writes land inside the disposable tree.
  void workspace;
  return environment;
}

async function createProbeDirectories(tempRoot) {
  const parent = tempRoot || os.tmpdir();
  await fs.mkdir(parent, { recursive: true });
  const tempDirectory = await fs.mkdtemp(path.join(parent, 'agenthub-chat-codex-probe-'));
  const codexHome = path.join(tempDirectory, 'codex-home');
  const workspace = path.join(tempDirectory, 'workspace');
  await Promise.all([
    fs.mkdir(codexHome, { recursive: true, mode: 0o700 }),
    fs.mkdir(workspace, { recursive: true, mode: 0o700 }),
    fs.mkdir(path.join(tempDirectory, 'tmp'), { recursive: true, mode: 0o700 }),
    fs.mkdir(path.join(codexHome, 'config'), { recursive: true, mode: 0o700 }),
    fs.mkdir(path.join(codexHome, 'data'), { recursive: true, mode: 0o700 }),
  ]);
  try {
    await fs.chmod(tempDirectory, 0o700);
  } catch {
    // chmod is not available on all supported filesystems. The directory is
    // still unique and is removed in finally below.
  }
  return { tempDirectory, codexHome, workspace };
}

function jsonLine(value) {
  return `${JSON.stringify(value)}\n`;
}

function requestParams(method, workspace) {
  switch (method) {
    case 'model/list':
      return {};
    case 'skills/list':
      return { cwds: [workspace], forceReload: false };
    case 'plugin/installed':
      return { cwds: [workspace], installSuggestionPluginNames: [] };
    case 'thread/start':
      // No turn/start or turn/steer is sent. `ephemeral` avoids leaving a
      // thread record in the user's actual Codex home (which is isolated too).
      return {
        cwd: workspace,
        ephemeral: true,
        sandbox: 'read-only',
        approvalPolicy: 'untrusted',
      };
    default:
      throw new Error(`no params for ${method}`);
  }
}

function initialParams() {
  return {
    clientInfo: {
      name: 'agenthub-chat-s0-probe',
      version: '0.1.0',
    },
  };
}

function resultForMethod(method, result, initializeResult) {
  const validation = validateMethodResult(method, result);
  if (!validation.valid) return methodFailure(validation.reason);
  const summary = { status: VERIFIED_PROTOCOL_ONLY };
  if (validation.itemCount !== undefined) summary.itemCount = validation.itemCount;
  if (method === INITIALIZE_METHOD) {
    initializeResult.cliVersion = validation.cliVersion;
    initializeResult.platform = validation.platform;
  }
  return summary;
}

/**
 * Run the protocol-only probe.
 *
 * `executable` and `executableArgs` are passed to child_process.spawn as
 * separate argv values. `spawnImpl` is exposed for deterministic tests and
 * defaults to node's spawn implementation.
 */
export async function runCodexProbe(options = {}) {
  const executable = options.executable ?? DEFAULT_EXECUTABLE;
  if (typeof executable !== 'string' || executable.length === 0) {
    throw new Error('executable must be a non-empty string');
  }
  const executableArgs = options.executableArgs
    ? [...options.executableArgs]
    : [...DEFAULT_EXECUTABLE_ARGS];
  if (!executableArgs.every((argument) => typeof argument === 'string')) {
    throw new Error('executableArgs must be strings');
  }

  const requestTimeoutMs = asPositiveInteger(options.requestTimeoutMs, DEFAULT_REQUEST_TIMEOUT_MS);
  const stderrLimitBytes = asPositiveInteger(options.stderrLimitBytes, DEFAULT_STDERR_LIMIT_BYTES);
  const stdoutLineLimitBytes = asPositiveInteger(
    options.stdoutLineLimitBytes,
    DEFAULT_STDOUT_LINE_LIMIT_BYTES,
  );
  const includePluginInstalled = options.includePluginInstalled !== false;
  const spawnImpl = options.spawnImpl ?? spawn;
  const methodsToProbe = includePluginInstalled
    ? [...PROBE_METHODS]
    : PROBE_METHODS.filter((method) => method !== 'plugin/installed');
  const directories = await createProbeDirectories(options.tempRoot);
  const environment = makeIsolatedEnvironment(directories);
  const methodResults = {
    [INITIALIZE_METHOD]: methodUnknown('not_started'),
    [INITIALIZED_NOTIFICATION]: methodUnknown('not_started'),
  };
  for (const method of methodsToProbe) methodResults[method] = methodUnknown('not_started');

  const counts = {
    requestsSent: 0,
    notificationsSent: 0,
    responsesSent: 0,
    responsesReceived: 0,
    notificationsReceived: 0,
    serverRequestsReceived: 0,
    invalidMessages: 0,
    stderrBytes: 0,
  };
  const wireOrder = [];
  const initializeResult = { cliVersion: UNKNOWN, platform: safePlatform(process.platform) };
  let child = null;
  let closed = false;
  let intentionallyStopping = false;
  let fatalCode = null;
  let nextRequestId = 1;
  let stdoutBuffer = '';
  const pending = new Map();
  let closeResolve;
  let processStopped = true;
  const closePromise = new Promise((resolve) => {
    closeResolve = resolve;
  });

  const failPending = (error) => {
    for (const entry of pending.values()) {
      clearTimeout(entry.timer);
      entry.reject(error);
    }
    pending.clear();
  };

  const setFatal = (code) => {
    if (!fatalCode) fatalCode = code;
    failPending(probeError(fatalCode));
  };

  const sendRaw = async (message, kind) => {
    if (!child?.stdin || child.stdin.destroyed || closed) throw probeError(fatalCode || 'process_closed');
    const payload = jsonLine(message);
    try {
      await new Promise((resolve, reject) => {
        child.stdin.write(payload, 'utf8', (error) => (error ? reject(error) : resolve()));
      });
    } catch {
      const code = fatalCode || 'stdin_error';
      setFatal(code);
      throw probeError(code);
    }
    if (kind === 'request') counts.requestsSent += 1;
    else if (kind === 'notification') counts.notificationsSent += 1;
    else counts.responsesSent += 1;
  };

  const sendServerRequestRejection = async (id) => {
    // Server-side approval/user-input requests are intentionally refused. A
    // protocol probe must never auto-approve or fabricate a user's answer.
    try {
      await sendRaw(
        {
          id,
          error: { code: -32601, message: 'protocol probe does not answer server requests' },
        },
        'response',
      );
    } catch {
      // The original fatal condition is retained and reported by the caller.
    }
  };

  const handleMessage = (message) => {
    if (!isObject(message)) {
      counts.invalidMessages += 1;
      setFatal('invalid_json');
      return;
    }

    const hasId = Object.hasOwn(message, 'id');
    const hasResult = Object.hasOwn(message, 'result');
    const hasError = Object.hasOwn(message, 'error');
    const hasMethod = typeof message.method === 'string';

    if (hasId && (hasResult || hasError)) {
      const entry = pending.get(message.id);
      if (!entry) {
        counts.invalidMessages += 1;
        setFatal('unexpected_response');
        return;
      }
      pending.delete(message.id);
      clearTimeout(entry.timer);
      counts.responsesReceived += 1;
      if (hasError) {
        const errorCode = isObject(message.error) ? safeErrorCode(message.error.code) : UNKNOWN;
        entry.resolve({ ok: false, errorCode });
      } else {
        entry.resolve({ ok: true, result: message.result });
      }
      return;
    }

    if (hasId && hasMethod) {
      counts.serverRequestsReceived += 1;
      void sendServerRequestRejection(message.id);
      return;
    }

    if (hasMethod && !hasId) {
      counts.notificationsReceived += 1;
      return;
    }

    counts.invalidMessages += 1;
    setFatal('invalid_json');
  };

  const handleStdout = (chunk) => {
    if (closed || fatalCode) return;
    stdoutBuffer += chunk;
    if (Buffer.byteLength(stdoutBuffer, 'utf8') > stdoutLineLimitBytes) {
      setFatal('stdout_limit');
      return;
    }
    let newlineIndex = stdoutBuffer.indexOf('\n');
    while (newlineIndex !== -1) {
      let line = stdoutBuffer.slice(0, newlineIndex);
      stdoutBuffer = stdoutBuffer.slice(newlineIndex + 1);
      if (line.endsWith('\r')) line = line.slice(0, -1);
      if (line.trim() !== '') {
        try {
          handleMessage(JSON.parse(line));
        } catch {
          counts.invalidMessages += 1;
          setFatal('invalid_json');
          return;
        }
      }
      if (fatalCode) return;
      newlineIndex = stdoutBuffer.indexOf('\n');
    }
    if (Buffer.byteLength(stdoutBuffer, 'utf8') > stdoutLineLimitBytes) setFatal('stdout_limit');
  };

  const request = async (method, params) => {
    if (fatalCode) throw probeError(fatalCode);
    const id = nextRequestId++;
    const response = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        setFatal('timeout');
        reject(probeError('timeout'));
      }, requestTimeoutMs);
      pending.set(id, { resolve, reject, timer });
    });
    // If writing the request fails, setFatal rejects this deferred promise
    // before request() can await it. Attach a handler now so that path never
    // creates an unhandled rejection.
    void response.catch(() => {});
    await sendRaw({ id, method, params }, 'request');
    return response;
  };

  const waitForClose = async (timeoutMs) => {
    if (closed) return true;
    let timeout;
    const timedOut = new Promise((resolve) => {
      timeout = setTimeout(() => resolve(false), timeoutMs);
    });
    const result = await Promise.race([closePromise.then(() => true), timedOut]);
    clearTimeout(timeout);
    return result;
  };

  const stopProcessTree = async () => {
    if (!child) return true;
    intentionallyStopping = true;
    try {
      if (!closed) child.stdin?.end();
    } catch {
      // Continue to process-tree termination below.
    }

    const pid = Number.isInteger(child.pid) ? child.pid : null;
    if (pid !== null && process.platform === 'win32') {
      // taskkill /T is the Windows equivalent of killing the detached POSIX
      // process group. Bound the helper so cleanup cannot hang forever.
      await new Promise((resolve) => {
        let killer;
        let timer;
        const finish = () => {
          clearTimeout(timer);
          resolve();
        };
        try {
          killer = spawn('taskkill', ['/PID', String(pid), '/T', '/F'], {
            stdio: 'ignore',
            windowsHide: true,
            shell: false,
          });
          timer = setTimeout(() => {
            try {
              killer.kill();
            } catch {
              // The helper may have exited between the timeout and kill.
            }
            finish();
          }, 500);
          killer.once('close', finish);
          killer.once('error', finish);
        } catch {
          finish();
        }
      });
    } else if (pid !== null) {
      // Always signal the process group, even if the root child already sent
      // close. A descendant can outlive its parent while retaining the pipes.
      try {
        process.kill(-pid, 'SIGTERM');
      } catch {
        try {
          child.kill('SIGTERM');
        } catch {
          // Process already exited.
        }
      }
      await waitForClose(150);
      try {
        process.kill(-pid, 'SIGKILL');
      } catch {
        try {
          child.kill('SIGKILL');
        } catch {
          // Process already exited.
        }
      }
    } else {
      try {
        child.kill('SIGKILL');
      } catch {
        // Process already exited.
      }
    }

    // Do not remove the temporary tree until the root process has confirmed
    // close. A failed kill is reported to the caller and leaves the tree for
    // safe manual cleanup instead of deleting files still in use.
    await waitForClose(350);
    return closed;
  };

  try {
    try {
      child = spawnImpl(executable, executableArgs, {
        cwd: directories.workspace,
        env: environment,
        shell: false,
        detached: process.platform !== 'win32',
        windowsHide: true,
        stdio: ['pipe', 'pipe', 'pipe'],
      });
    } catch {
      fatalCode = 'spawn_error';
    }

    if (child) {
      child.stdout?.setEncoding?.('utf8');
      child.stdout?.on('data', handleStdout);
      child.stderr?.on('data', (chunk) => {
        counts.stderrBytes += Buffer.byteLength(chunk);
        if (counts.stderrBytes > stderrLimitBytes) setFatal('stderr_limit');
      });
      child.once('error', () => {
        if (!intentionallyStopping) setFatal('spawn_error');
      });
      child.once('close', () => {
        closed = true;
        if (!intentionallyStopping && !fatalCode) {
          if (stdoutBuffer.trim() !== '') setFatal('incomplete_json');
          else setFatal('unexpected_exit');
        }
        if (!intentionallyStopping) failPending(probeError(fatalCode || 'unexpected_exit'));
        closeResolve();
      });

      wireOrder.push(INITIALIZE_METHOD);
      try {
        const response = await request(INITIALIZE_METHOD, initialParams());
        if (!response.ok) {
          methodResults[INITIALIZE_METHOD] = methodFailure('upstream_error', {
            errorCode: response.errorCode,
          });
        } else {
          methodResults[INITIALIZE_METHOD] = resultForMethod(
            INITIALIZE_METHOD,
            response.result,
            initializeResult,
          );
        }
      } catch (error) {
        methodResults[INITIALIZE_METHOD] = methodFailure(error.code || fatalCode || 'request_failed');
      }

      if (methodResults[INITIALIZE_METHOD].status === VERIFIED_PROTOCOL_ONLY && !fatalCode) {
        wireOrder.push(INITIALIZED_NOTIFICATION);
        try {
          await sendRaw({ method: INITIALIZED_NOTIFICATION }, 'notification');
          methodResults[INITIALIZED_NOTIFICATION] = { status: VERIFIED_PROTOCOL_ONLY };
        } catch (error) {
          methodResults[INITIALIZED_NOTIFICATION] = methodFailure(
            error.code || fatalCode || 'request_failed',
          );
        }
      } else {
        methodResults[INITIALIZED_NOTIFICATION] = methodUnknown('initialize_failed');
      }

      for (const method of methodsToProbe) {
        if (fatalCode) {
          methodResults[method] = methodUnknown(fatalCode);
          continue;
        }
        if (methodResults[INITIALIZED_NOTIFICATION].status !== VERIFIED_PROTOCOL_ONLY) {
          methodResults[method] = methodUnknown('initialized_failed');
          continue;
        }
        wireOrder.push(method);
        try {
          const response = await request(method, requestParams(method, directories.workspace));
          if (!response.ok) {
            methodResults[method] = methodFailure('upstream_error', {
              errorCode: response.errorCode,
            });
          } else {
            methodResults[method] = resultForMethod(method, response.result, initializeResult);
          }
        } catch (error) {
          methodResults[method] = methodFailure(error.code || fatalCode || 'request_failed');
        }
      }
    }
  } finally {
    // Keep cleanup errors in the report so a still-running process never
    // gets hidden behind a successful temporary-directory removal.
    processStopped = await stopProcessTree().catch(() => false);
  }

  let cleanup = processStopped ? 'ok' : 'failed';
  if (processStopped) {
    try {
      await fs.rm(directories.tempDirectory, { recursive: true, force: true });
    } catch {
      cleanup = 'failed';
    }
  }
  return buildSummary({
    methodResults,
    counts,
    wireOrder,
    initializeResult,
    fatalCode,
    cleanup,
  });
}

function buildSummary({ methodResults, counts, wireOrder, initializeResult, fatalCode, cleanup }) {
  const statuses = Object.values(methodResults).map((entry) => entry.status);
  const allVerified = statuses.every((status) => status === VERIFIED_PROTOCOL_ONLY);
  const hasFailure = statuses.some((status) => status === 'failed');
  const probeFailed = cleanup !== 'ok' || Boolean(fatalCode) || hasFailure;
  return {
    schema: 'agenthub-chat-codex-probe/v1',
    status: allVerified && cleanup === 'ok' ? 'ok' : probeFailed ? 'failed' : 'partial',
    cliVersion: initializeResult.cliVersion,
    os: initializeResult.platform,
    methods: methodResults,
    counts,
    wireOrder,
    unverified: [...UNVERIFIED_CAPABILITIES],
    cleanup,
  };
}

function parseCliArgs(argv) {
  const options = {};
  const nextValue = (index, argument) => {
    const value = argv[index + 1];
    if (value === undefined || value.startsWith('--')) throw new Error(`${argument} requires a value`);
    return value;
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === '--help' || argument === '-h') return { help: true };
    if (argument === '--executable') {
      options.executable = nextValue(index, argument);
      index += 1;
    } else if (argument === '--timeout-ms') {
      options.requestTimeoutMs = Number(nextValue(index, argument));
      index += 1;
    } else if (argument === '--stderr-limit-bytes') {
      options.stderrLimitBytes = Number(nextValue(index, argument));
      index += 1;
    } else if (argument === '--stdout-line-limit-bytes') {
      options.stdoutLineLimitBytes = Number(nextValue(index, argument));
      index += 1;
    } else if (argument === '--skip-plugin-installed') {
      options.includePluginInstalled = false;
    } else {
      throw new Error(`unknown option ${argument}`);
    }
  }
  return options;
}

function printHelp() {
  console.log(`Usage: node scripts/chat-codex-probe.mjs [options]

Probe Codex app-server's read-only JSON-RPC protocol without starting a model turn.

Options:
  --executable PATH             Executable to spawn (default: codex)
  --timeout-ms N                Per-request timeout (default: 8000)
  --stderr-limit-bytes N        Maximum stderr bytes (default: 65536)
  --stdout-line-limit-bytes N   Maximum protocol line bytes (default: 1048576)
  --skip-plugin-installed       Omit the schema-confirmed plugin/installed check
  -h, --help                    Show this help

The JSON report contains only protocol summaries and intentionally unverified capabilities.`);
}

async function main() {
  let options;
  try {
    options = parseCliArgs(process.argv.slice(2));
  } catch {
    console.error('chat-codex-probe: invalid arguments');
    process.exitCode = 2;
    return;
  }
  if (options.help) {
    printHelp();
    return;
  }
  let summary;
  try {
    summary = await runCodexProbe(options);
  } catch {
    console.error('chat-codex-probe: probe could not start');
    process.exitCode = 1;
    return;
  }
  console.log(JSON.stringify(summary, null, 2));
  if (summary.status === 'failed') process.exitCode = 1;
}

const currentFile = path.resolve(fileURLToPath(import.meta.url));
const invokedFile = process.argv[1] ? path.resolve(process.argv[1]) : '';
if (currentFile === invokedFile) void main();
