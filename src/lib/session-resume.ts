import { detectHostPlatform, type HostPlatform } from '@/lib/platform-detect';

const MAX_SESSION_ID_LEN = 512;

/** Planned native resume argv (`argv[0]` is the CLI name). */
export type NativeResumePlan = {
  agentId: string;
  argv: string[];
};

/**
 * Official **interactive TUI** resume argv for a listed native session id.
 * Flags match herdr `agent_resume.rs` (`claude --resume`, `codex resume`, …).
 * Chat subsequent turns use print-mode resume in core, not this helper.
 */
export function planNativeResume(
  agentId: string,
  sessionId: string | null | undefined,
  host: HostPlatform = detectHostPlatform(),
): NativeResumePlan | null {
  const id = validSessionId(sessionId);
  if (!id) return null;
  const argv = resumeArgv(agentId, id, host);
  if (!argv) return null;
  return { agentId, argv };
}

export function formatResumeCommand(argv: readonly string[]): string {
  return argv.map(quoteArg).join(' ');
}

export function nativeResumeCommand(
  agentId: string,
  sessionId: string | null | undefined,
  host?: HostPlatform,
): string | null {
  const plan = planNativeResume(agentId, sessionId, host);
  return plan ? formatResumeCommand(plan.argv) : null;
}

function resumeArgv(agentId: string, sessionId: string, host: HostPlatform): string[] | null {
  switch (agentId) {
    case 'claude':
      return ['claude', '--resume', sessionId];
    case 'codex':
      return ['codex', 'resume', sessionId];
    case 'kimi':
      return ['kimi', '--session', sessionId];
    case 'grok':
      return ['grok', '--resume', sessionId];
    case 'pi':
      return ['pi', '--session', sessionId];
    case 'cursor':
      return [host === 'windows' ? 'cursor-agent.cmd' : 'cursor-agent', '--resume', sessionId];
    default:
      return null;
  }
}

function validSessionId(value: string | null | undefined): string | null {
  const id = value?.trim() ?? '';
  if (!id || id.length > MAX_SESSION_ID_LEN || /[\u0000-\u001f\u007f]/.test(id)) {
    return null;
  }
  return id;
}

function quoteArg(value: string): string {
  if (/^[A-Za-z0-9._:/=+-]+$/.test(value)) return value;
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}
