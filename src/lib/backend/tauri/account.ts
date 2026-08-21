import {
  normalizeAuthState,
  type AccountPort,
  type AuthState,
  type OAuthStartInfo,
  type OAuthWaitInfo,
} from '@/lib/backend/contracts';
import {
  mapCoreAccount,
  type CoreAccount,
  type CoreAccountSwitchResult,
} from '@/lib/backend/contracts/account-map';
import { unsupportedError } from '@/lib/backend/contracts/errors';
import { OAUTH_WAIT_TIMEOUT_SECS } from '@/lib/backend/contracts/oauth-constants';
import type { AgentId } from '@/lib/types';
import { logger } from '@/lib/logger';
import { invoke } from './invoke';

const log = logger.scope('backend:tauri:account');

export function createTauriAccountPort(): AccountPort {
  return {
    async listAccounts(agentId) {
      try {
        const rows = await invoke<CoreAccount[]>('list_accounts', {
          agentId: agentId ?? null,
        });
        return rows.map(mapCoreAccount);
      } catch (e) {
        log.error('list_accounts failed', e);
        throw e;
      }
    },

    async probeLiveAuth(agentId) {
      try {
        const raw = await invoke<AuthState & { agentId?: AgentId }>('probe_live_auth', { agentId });
        return normalizeAuthState(raw, agentId);
      } catch (e) {
        log.error('probe_live_auth failed', e);
        throw e;
      }
    },

    async switchAccount(agentId, accountId) {
      try {
        await invoke<CoreAccountSwitchResult>('switch_account', {
          agentId,
          idOrLabel: accountId,
        });
      } catch (e) {
        log.error('switch_account failed', e);
        throw e;
      }
    },

    async undoSwitchAccount(agentId) {
      try {
        return await invoke<boolean>('undo_switch_account', { agentId });
      } catch (e) {
        log.error('undo_switch_account failed', e);
        throw e;
      }
    },

    async addApiKeyAccount(agentId, key, label, envKey, productMarker) {
      try {
        const row = await invoke<CoreAccount>('add_api_key_account', {
          agentId,
          key,
          label: label?.trim() ? label.trim() : null,
          envKey: envKey?.trim() ? envKey.trim() : null,
          productMarker: productMarker?.trim() ? productMarker.trim() : null,
        });
        return mapCoreAccount(row);
      } catch (e) {
        log.error('add_api_key_account failed', e);
        throw e;
      }
    },

    async updateApiKeyAccount(agentId, accountId, opts) {
      try {
        const row = await invoke<CoreAccount>('update_api_key_account', {
          agentId,
          idOrLabel: accountId,
          label: opts.label?.trim() ? opts.label.trim() : null,
          key: opts.key?.trim() ? opts.key.trim() : null,
        });
        return mapCoreAccount(row);
      } catch (e) {
        log.error('update_api_key_account failed', e);
        throw e;
      }
    },

    async importCurrentLogin(agentId) {
      try {
        const row = await invoke<CoreAccount>('import_account_live', {
          agentId,
          name: null,
        });
        return mapCoreAccount(row);
      } catch (e) {
        log.error('import_account_live failed', e);
        throw e;
      }
    },

    async oauthSupported(agentId) {
      return invoke<boolean>('oauth_supported', { agentId });
    },

    async listOAuthOptions(agentId) {
      return invoke('oauth_list_options', { agentId });
    },

    async startOAuth(agentId, openBrowser = true, providerKey) {
      return invoke<OAuthStartInfo>('oauth_start', {
        agentId,
        openBrowser,
        providerKey: providerKey ?? null,
      });
    },

    async waitOAuth(state, timeoutSecs = OAUTH_WAIT_TIMEOUT_SECS) {
      return invoke<OAuthWaitInfo>('oauth_wait', {
        oauthState: state,
        timeoutSecs,
      });
    },

    async finishOAuth(state) {
      const row = await invoke<CoreAccount>('oauth_complete', {
        oauthState: state,
      });
      return mapCoreAccount(row);
    },

    async startDeviceOAuth(agentId, providerKey) {
      return invoke('oauth_device_start', { agentId, providerKey });
    },

    async pollDeviceOAuth(state) {
      return invoke('oauth_device_poll', { oauthState: state });
    },

    async finishDeviceOAuth(state) {
      const row = await invoke<CoreAccount>('oauth_device_complete', {
        oauthState: state,
      });
      return mapCoreAccount(row);
    },

    async completeOAuth(agentId: AgentId, providerKey) {
      const supported = await this.oauthSupported(agentId);
      if (!supported) {
        throw unsupportedError(
          'OAuth 浏览器授权',
          '该 Agent 未配置 OAuth；请使用「导入当前账号」或「添加 API Key」',
        );
      }
      const options = await this.listOAuthOptions(agentId);
      const key = providerKey ?? (options.length === 1 ? options[0]!.id : null);
      const opt = key ? options.find((o) => o.id === key) : undefined;
      if (opt?.flow === 'deviceCode') {
        const start = await this.startDeviceOAuth(agentId, opt.id);
        const deadline = Date.now() + (start.expiresInSecs || 900) * 1000;
        while (Date.now() < deadline) {
          await new Promise((r) => setTimeout(r, (start.intervalSecs || 5) * 1000));
          const poll = await this.pollDeviceOAuth(start.state);
          if (poll.status === 'complete') return this.finishDeviceOAuth(start.state);
          if (poll.status === 'failed' || poll.status === 'expired') {
            throw unsupportedError('OAuth 授权', poll.error ?? '设备码授权失败');
          }
        }
        throw unsupportedError('OAuth 授权', '设备码授权超时');
      }
      const start = await this.startOAuth(agentId, true, key);
      const wait = await this.waitOAuth(start.state, OAUTH_WAIT_TIMEOUT_SECS);
      if (wait.status === 'failed') {
        throw unsupportedError('OAuth 授权', wait.error ?? '授权失败');
      }
      return this.finishOAuth(start.state);
    },

    async deleteAccount(agentId, accountId) {
      try {
        await invoke('delete_account', {
          agentId,
          idOrLabel: accountId,
        });
      } catch (e) {
        log.error('delete_account failed', e);
        throw e;
      }
    },

    async refreshToken(agentId, accountId) {
      try {
        await invoke<CoreAccount>('refresh_account_token', {
          agentId,
          idOrLabel: accountId,
        });
      } catch (e) {
        log.error('refresh_account_token failed', e);
        throw e;
      }
    },

    async refreshQuota(agentId, accountId) {
      try {
        const raw = await invoke<CoreAccount>('refresh_account_quota', {
          agentId,
          idOrLabel: accountId,
        });
        return mapCoreAccount(raw);
      } catch (e) {
        log.error('refresh_account_quota failed', e);
        throw e;
      }
    },
  };
}
