/** Sub2API management API + local session shapes. */

export const SUB2API_DEFAULT_SITE_URL = 'https://v2.pincc.ai';

export type Sub2ApiEnvelope<T> = {
  code: number;
  message: string;
  data: T;
  /** Present on some error envelopes (e.g. TENCENT_CAPTCHA_VERIFICATION_FAILED). */
  reason?: string;
};

export type Sub2ApiUser = {
  id: number;
  username?: string;
  email?: string;
  display_name?: string | null;
  role?: string;
  status?: string;
};

/** Public settings used for login/captcha. Extra keys ignored. */
export type Sub2ApiPublicSettings = {
  api_base_url?: string;
  version?: string;
  site_name?: string;
  turnstile_enabled?: boolean;
  turnstile_site_key?: string;
  tencent_captcha_enabled?: boolean;
  tencent_captcha_app_id?: string;
  tencent_captcha_region?: string;
  aliyun_captcha_enabled?: boolean;
  aliyun_captcha_scene_id?: string;
  aliyun_captcha_prefix?: string;
  aliyun_captcha_region?: string;
  [key: string]: unknown;
};

export type Sub2ApiLoginRequest = {
  email: string;
  password: string;
  turnstile_token?: string;
  tencent_captcha_ticket?: string;
  tencent_captcha_randstr?: string;
};

export type Sub2ApiAuthTokens = {
  access_token: string;
  refresh_token?: string;
  expires_in?: number;
  token_type?: string;
  user?: Sub2ApiUser;
};

export type Sub2ApiTotpLoginResponse = {
  requires_2fa: true;
  temp_token?: string;
  user_email_masked?: string;
};

export type Sub2ApiLoginResult = Sub2ApiAuthTokens | Sub2ApiTotpLoginResponse;

export type Sub2ApiLogin2FARequest = {
  temp_token: string;
  totp_code: string;
};

/** User-bindable group from GET /groups/available. */
export type Sub2ApiGroup = {
  id: number;
  name: string;
  platform?: string;
  description?: string | null;
  status?: string;
  rate_multiplier?: number;
};

/** Embedded group when /keys returns group as an object (Sub2API / PinCC). */
export type Sub2ApiKeyGroup = {
  id?: number;
  name?: string;
  platform?: string;
  description?: string | null;
  status?: string;
  models_list_config?: { enabled?: boolean; models?: string[] } | null;
  [key: string]: unknown;
};

export type Sub2ApiKey = {
  id: number;
  user_id?: number;
  key: string;
  name: string;
  group_id?: number | null;
  /** Group display name when the API provides it. */
  group_name?: string | null;
  /** String label or embedded object (Sub2API / PinCC). */
  group?: string | Sub2ApiKeyGroup | null;
  status: string;
  created_at?: string;
  updated_at?: string;
  expires_at?: string | null;
  last_used_at?: string | null;
  /** Allowed models — array or comma-separated string depending on relay. */
  models?: string[] | string | null;
  /** USD on Sub2API; 0 often = unlimited. */
  quota?: number | null;
  /** PinCC / Sub2API used amount. */
  quota_used?: number | null;
  /** NewAPI-style aliases. */
  used_quota?: number | null;
  remain_quota?: number | null;
  remaining?: number | null;
  unlimited_quota?: boolean | null;
  ip_whitelist?: string[] | null;
  ip_blacklist?: string[] | null;
  rate_limit_5h?: number | null;
  rate_limit_1d?: number | null;
  rate_limit_7d?: number | null;
  usage_5h?: number | null;
  usage_1d?: number | null;
  usage_7d?: number | null;
  /** Extra fields from Sub2API / PinCC / NewAPI-style relays — ignored safely. */
  [key: string]: unknown;
};

export type Sub2ApiKeyList = {
  items: Sub2ApiKey[];
  total?: number;
  page?: number;
  page_size?: number;
  pages?: number;
};

/** PUT /keys/:id — omit a field to leave it unchanged. */
export type Sub2ApiKeyPatch = {
  name?: string;
  group_id?: number | null;
  status?: 'active' | 'inactive';
  ip_whitelist?: string[];
  ip_blacklist?: string[];
  quota?: number;
  /** ISO timestamp; empty string clears expiration (Sub2API). */
  expires_at?: string | null;
  reset_quota?: boolean;
  rate_limit_5h?: number;
  rate_limit_1d?: number;
  rate_limit_7d?: number;
  reset_rate_limit_usage?: boolean;
};

/** Persisted JWT session only — gateway API Keys live in Connections. */
export type Sub2ApiSession = {
  siteUrl: string;
  gatewayBaseUrl: string;
  accessToken: string;
  refreshToken?: string;
  expiresAt?: number;
  user?: Sub2ApiUser | null;
};

export type Sub2ApiAuthContext = {
  siteUrl: string;
  accessToken: string;
};

export type Sub2ApiCaptchaKind = 'none' | 'turnstile' | 'tencent' | 'aliyun';

export type Sub2ApiCaptchaProof = {
  turnstile_token?: string;
  tencent_captcha_ticket?: string;
  tencent_captcha_randstr?: string;
};
