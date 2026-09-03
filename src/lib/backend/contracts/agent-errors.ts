import type { InstallOutcome } from './install-types';
import type { AgentKey, RuntimeId } from '@/lib/types';

export class EnvNotReadyError extends Error {
  readonly code = 'env.not_ready';
  readonly missing: RuntimeId[];
  readonly channel: string;
  readonly agent: AgentKey;

  constructor(agent: AgentKey, channel: string, missing: RuntimeId[]) {
    super(
      `环境未就绪:安装 ${agent}(${channel}) 需要 ${missing.join(', ')}。请先安装运行环境或使用 --install-deps。`,
    );
    this.name = 'EnvNotReadyError';
    this.agent = agent;
    this.channel = channel;
    this.missing = missing;
  }
}

export class InstallFailedError extends Error {
  readonly logs: string[];
  readonly outcome: InstallOutcome;

  constructor(outcome: InstallOutcome) {
    super(outcome.message);
    this.name = 'InstallFailedError';
    this.logs = outcome.logs;
    this.outcome = outcome;
  }
}

export class RuntimeInstallFailedError extends Error {
  readonly logs: string[];
  readonly outcome: InstallOutcome;

  constructor(outcome: InstallOutcome) {
    super(outcome.message);
    this.name = 'RuntimeInstallFailedError';
    this.logs = outcome.logs;
    this.outcome = outcome;
  }
}
