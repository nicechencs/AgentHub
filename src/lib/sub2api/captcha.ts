/**
 * Sub2API captcha script loaders (Turnstile / Tencent / Aliyun).
 * Never logs captcha tokens.
 */

export type TencentCaptchaRegion = 'cn' | 'intl';

export type TencentCaptchaProof = { ticket: string; randstr: string };

export type TencentCaptchaResult = {
  ret: number;
  ticket?: string | null;
  randstr?: string | null;
  errorCode?: number;
};

type TencentCaptchaInstance = { show(): void; destroy(): void };

type TencentCaptchaCtor = {
  new (
    appIdOrEl: string | HTMLElement,
    callbackOrAppId: ((result: TencentCaptchaResult) => void) | string,
    optionsOrCallback?: Record<string, unknown> | ((result: TencentCaptchaResult) => void),
    options?: Record<string, unknown>,
  ): TencentCaptchaInstance;
};

declare global {
  interface Window {
    turnstile?: {
      render: (
        container: HTMLElement,
        options: {
          sitekey: string;
          callback: (token: string) => void;
          'expired-callback'?: () => void;
          'error-callback'?: () => void;
          theme?: 'light' | 'dark' | 'auto';
          size?: 'normal' | 'compact' | 'flexible';
        },
      ) => string;
      reset: (widgetId?: string) => void;
      remove: (widgetId?: string) => void;
    };
    onTurnstileLoad?: () => void;
    TencentCaptcha?: TencentCaptchaCtor;
    TCaptchaGlobal?: boolean;
    initAliyunCaptcha?: (options: {
      SceneId: string;
      prefix: string;
      mode: 'popup' | 'embed';
      element: string;
      button: string;
      captchaVerifyCallback: (param: string) => { captchaResult: boolean };
      onBizResultCallback: (bizResult: boolean) => void;
      getInstance: (instance: unknown) => void;
      language?: string;
    }) => void;
    AliyunCaptchaConfig?: { region: string; prefix: string };
  }
}

const TURNSTILE_SRC =
  'https://challenges.cloudflare.com/turnstile/v0/api.js?onload=onTurnstileLoad';
const TENCENT_SRC: Record<TencentCaptchaRegion, string> = {
  cn: 'https://turing.captcha.qcloud.com/TJCaptcha.js',
  intl: 'https://ca.turing.captcha.qcloud.com/TJNCaptcha-global.js',
};
const ALIYUN_SRC = 'https://o.alicdn.com/captcha-frontend/aliyunCaptcha/AliyunCaptcha.js';

let turnstilePromise: Promise<void> | null = null;
let tencentPromise: Promise<TencentCaptchaCtor> | null = null;
let tencentLoadedRegion: TencentCaptchaRegion | null = null;
let aliyunPromise: Promise<void> | null = null;

export function normalizeTencentCaptchaRegion(value?: string | null): TencentCaptchaRegion {
  return value === 'intl' ? 'intl' : 'cn';
}

export function loadTurnstileScript(): Promise<void> {
  if (typeof window === 'undefined') return Promise.reject(new Error('no window'));
  if (window.turnstile) return Promise.resolve();
  if (turnstilePromise) return turnstilePromise;

  turnstilePromise = new Promise((resolve, reject) => {
    const existing = document.querySelector('script[src*="turnstile"]');
    if (existing) {
      window.onTurnstileLoad = () => resolve();
      if (window.turnstile) resolve();
      return;
    }
    const script = document.createElement('script');
    script.src = TURNSTILE_SRC;
    script.async = true;
    script.defer = true;
    window.onTurnstileLoad = () => resolve();
    script.onerror = () => {
      turnstilePromise = null;
      reject(new Error('Failed to load Turnstile'));
    };
    document.head.appendChild(script);
  });
  return turnstilePromise;
}

export function loadTencentCaptcha(
  region: TencentCaptchaRegion = 'cn',
): Promise<TencentCaptchaCtor> {
  if (typeof window === 'undefined') return Promise.reject(new Error('no window'));
  if (window.TencentCaptcha && tencentLoadedRegion === region) {
    return Promise.resolve(window.TencentCaptcha);
  }
  if (tencentPromise && tencentLoadedRegion === region) return tencentPromise;

  tencentLoadedRegion = region;
  tencentPromise = new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = TENCENT_SRC[region];
    script.async = true;
    script.onload = () => {
      if (window.TencentCaptcha) {
        resolve(window.TencentCaptcha);
        return;
      }
      tencentPromise = null;
      tencentLoadedRegion = null;
      reject(new Error('Tencent Captcha SDK unavailable'));
    };
    script.onerror = () => {
      tencentPromise = null;
      tencentLoadedRegion = null;
      reject(new Error('Failed to load Tencent Captcha'));
    };
    document.head.appendChild(script);
  });
  return tencentPromise;
}

export function loadAliyunCaptchaScript(): Promise<void> {
  if (typeof window === 'undefined') return Promise.reject(new Error('no window'));
  if (window.initAliyunCaptcha) return Promise.resolve();
  if (aliyunPromise) return aliyunPromise;

  aliyunPromise = new Promise((resolve, reject) => {
    const existing = document.querySelector('script[src*="aliyunCaptcha/AliyunCaptcha"]');
    if (existing) {
      existing.addEventListener('load', () => resolve());
      existing.addEventListener('error', () => {
        aliyunPromise = null;
        reject(new Error('Failed to load Aliyun captcha'));
      });
      if (window.initAliyunCaptcha) resolve();
      return;
    }
    const script = document.createElement('script');
    script.src = ALIYUN_SRC;
    script.async = true;
    script.onload = () => resolve();
    script.onerror = () => {
      aliyunPromise = null;
      reject(new Error('Failed to load Aliyun captcha'));
    };
    document.head.appendChild(script);
  });
  return aliyunPromise;
}

/** Popup Tencent captcha and return ticket/randstr, or null if cancelled. */
export async function verifyTencentCaptcha(input: {
  appId: string;
  region?: string | null;
  langZh?: boolean;
}): Promise<TencentCaptchaProof | null> {
  const region = normalizeTencentCaptchaRegion(input.region);
  const Ctor = await loadTencentCaptcha(region);
  return new Promise((resolve, reject) => {
    let settled = false;
    const finish = (fn: () => void) => {
      if (settled) return;
      settled = true;
      fn();
    };
    try {
      const userLanguage = input.langZh ? 'zh-cn' : 'en';
      const instance = new Ctor(
        input.appId,
        (result) => {
          if (result.ret === 2) {
            finish(() => resolve(null));
            return;
          }
          const ticket = result.ticket?.trim() || '';
          const randstr = result.randstr?.trim() || '';
          if (!ticket || !randstr || ticket.startsWith('trerror_') || result.errorCode !== undefined) {
            finish(() => reject(new Error('Tencent Captcha verification failed')));
            return;
          }
          finish(() => {
            try {
              instance.destroy();
            } catch {
              /* ignore */
            }
            resolve({ ticket, randstr });
          });
        },
        { userLanguage },
      );
      instance.show();
    } catch (e) {
      reject(e instanceof Error ? e : new Error(String(e)));
    }
  });
}

export function __resetCaptchaLoadersForTests(): void {
  turnstilePromise = null;
  tencentPromise = null;
  tencentLoadedRegion = null;
  aliyunPromise = null;
}
