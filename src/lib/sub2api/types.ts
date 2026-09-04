/** Sub2API management API + local session shapes. */

export const SUB2API_DEFAULT_SITE_URL = 'https://v2.pincc.ai';

export type Sub2ApiEnvelope<T> = {
  code: number;
  message: string;
  data: T;
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

export type Sub2ApiKey = {
  id: number;
  user_id?: number;
  key: string;
  name: string;
  group_id?: number | null;
  status: string;
  created_at?: string;
  updated_at?: string;
};

export type Sub2ApiKeyList = {
  items: Sub2ApiKey[];
  total?: number;
  page?: number;
  page_size?: number;
  pages?: number;
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
