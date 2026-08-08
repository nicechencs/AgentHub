import type { AccountPort, OAuthStartInfo, OAuthWaitInfo } from '@/lib/backend/contracts';
import {
  mapCoreAccount,
  type CoreAccount,
  type CoreAccountSwitchResult,
} from '@/lib/backend/contracts/account-map';
import { unsupportedError } from '@/lib/backend/contracts/errors';
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

    async undoSwitchAccount() {
      return false;
    },

    async addApiKeyAccount(agentId, key, label, envKey) {
      try {
        const row = await invoke<CoreAccount>('add_api_key_account', {
          agentId,
          key,
          label: label?.trim() ? label.trim() : null,
          envKey: envKey?.trim() ? envKey.trim() : null,
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

    async startOAuth(agentId, openBrowser = true) {
      return invoke<OAuthStartInfo>('oauth_start', {
        agentId,
        openBrowser,
      });
    },

    async waitOAuth(state, timeoutSecs = 120) {
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

    async completeOAuth(agentId: AgentId) {
      const supported = await this.oauthSupported(agentId);
      if (!supported) {
        throw unsupportedError(
          'OAuth 浏览器授权',
          '该 Agent 未配置 PKCE；请使用「导入当前账号」或「添加 API Key」',
        );
      }
      const start = await this.startOAuth(agentId, true);
      const wait = await this.waitOAuth(start.state, 120);
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
  };
}
