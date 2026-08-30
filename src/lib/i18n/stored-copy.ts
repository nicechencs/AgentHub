import {
  authHealthLabel,
  type AuthHealth,
} from '@/lib/backend/contracts/auth-state';
import type { TranslateFn } from '@/lib/i18n';

const STORED_HEALTH_LABEL: Record<string, AuthHealth> = {
  已验证: 'verified',
  Verified: 'verified',
  可续期: 'renewable',
  Renewable: 'renewable',
  已配置: 'configured',
  Configured: 'configured',
  需要重新登录: 'needs_login',
  'Sign in again': 'needs_login',
  状态未知: 'unknown',
  Unknown: 'unknown',
  未登录: 'missing',
  'Not signed in': 'missing',
};

const UNVERIFIED_SUFFIX = /[·，,]\s*(尚未验证|未验证|unverified|not verified)\s*$/iu;

/** Backend/store rows keep Chinese literals. Remap at display time when `t` is set. */
export function localizeStoredUiCopy(raw: string, t?: TranslateFn): string {
  if (!t || !raw) return raw;
  const stripped = raw.replace(UNVERIFIED_SUFFIX, '').trim() || raw;
  if (stripped === '未配置' || stripped === 'Not configured') {
    return t('dashboard.overview.unconfigured');
  }
  if (stripped === '已登录' || stripped === 'Signed in') {
    return t('chat.connection.signedIn');
  }
  if (stripped === '未检测登录态' || stripped === 'Login not detected') {
    return t('kind.health.unknown');
  }
  if (stripped === '本机路由' || stripped === 'Local route') {
    return t('kind.route.localRoute');
  }
  if (stripped.startsWith('本机路由 · ')) {
    return `${t('kind.route.localRoute')} · ${stripped.slice('本机路由 · '.length)}`;
  }
  if (stripped.startsWith('Local route · ')) {
    return `${t('kind.route.localRoute')} · ${stripped.slice('Local route · '.length)}`;
  }
  const health = STORED_HEALTH_LABEL[stripped];
  if (health) return authHealthLabel(health, t);
  return stripped;
}

export function localizeSkillMarketDescription(raw: string, t?: TranslateFn): string {
  if (!t || !raw) return raw;
  const match = raw.match(/^来自 (.+) · (.+) 次安装$/);
  if (match) {
    return t('skills.market.fromInstalls', { source: match[1], count: match[2] });
  }
  const en = raw.match(/^From (.+) · (.+) installs$/i);
  if (en) {
    return t('skills.market.fromInstalls', { source: en[1], count: en[2] });
  }
  return raw;
}
