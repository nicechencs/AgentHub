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

export type Sub2ApiPublicSettings = {
  api_base_url?: string;
  version?: string;
  [key: string]: unknown;
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
