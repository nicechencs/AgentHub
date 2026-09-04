/**
 * Captcha for native Sub2API login.
 * - Turnstile: load Cloudflare script and embed widget.
 * - Tencent / Aliyun: action-triggered on submit (scripts loaded then); UI shows status.
 */
import * as React from 'react';
import { Button } from '@/components/ui/button';
import {
  loadAliyunCaptchaScript,
  loadTurnstileScript,
  verifyTencentCaptcha,
} from '@/lib/sub2api/captcha';
import type { Sub2ApiCaptchaKind, Sub2ApiCaptchaProof, Sub2ApiPublicSettings } from '@/lib/sub2api';
import { resolveCaptchaKind } from '@/lib/sub2api/client';
import { cn } from '@/lib/utils';

export type Sub2ApiCaptchaLabels = {
  turnstileLoading: string;
  turnstileFailed: string;
  actionReady: string;
  actionVerified: string;
  actionFailed: string;
  actionNeeded: string;
};

type Props = {
  settings: Sub2ApiPublicSettings | null;
  langZh?: boolean;
  labels: Sub2ApiCaptchaLabels;
  onProofChange: (proof: Sub2ApiCaptchaProof | null) => void;
  className?: string;
};

export type Sub2ApiCaptchaHandle = {
  /** Ensure proof is ready before POST /auth/login. Returns null if user cancelled. */
  ensureProof: () => Promise<Sub2ApiCaptchaProof | null>;
  reset: () => void;
  kind: () => Sub2ApiCaptchaKind;
};

export const Sub2ApiCaptcha = React.forwardRef<Sub2ApiCaptchaHandle, Props>(
  function Sub2ApiCaptcha({ settings, langZh, labels, onProofChange, className }, ref) {
    const kind = resolveCaptchaKind(settings);
    const containerRef = React.useRef<HTMLDivElement>(null);
    const widgetIdRef = React.useRef<string | null>(null);
    const [status, setStatus] = React.useState<'idle' | 'loading' | 'ready' | 'verified' | 'error'>(
      'idle',
    );
    const [proof, setProof] = React.useState<Sub2ApiCaptchaProof | null>(null);
    const aliyunIds = React.useRef({
      button: `aliyun-captcha-btn-${Math.random().toString(36).slice(2, 8)}`,
      element: `aliyun-captcha-el-${Math.random().toString(36).slice(2, 8)}`,
    });
    const aliyunCached = React.useRef<string | null>(null);

    const publish = React.useCallback(
      (next: Sub2ApiCaptchaProof | null) => {
        setProof(next);
        onProofChange(next);
      },
      [onProofChange],
    );

    const reset = React.useCallback(() => {
      aliyunCached.current = null;
      publish(null);
      if (kind === 'turnstile' && window.turnstile && widgetIdRef.current) {
        try {
          window.turnstile.reset(widgetIdRef.current);
        } catch {
          /* ignore */
        }
      }
      setStatus(kind === 'none' ? 'idle' : 'ready');
    }, [kind, publish]);

    React.useEffect(() => {
      let cancelled = false;
      publish(null);
      widgetIdRef.current = null;

      if (kind === 'none') {
        setStatus('idle');
        return;
      }

      if (kind === 'turnstile') {
        const siteKey = settings?.turnstile_site_key?.trim() || '';
        if (!siteKey) {
          setStatus('error');
          return;
        }
        setStatus('loading');
        void loadTurnstileScript()
          .then(() => {
            if (cancelled || !containerRef.current || !window.turnstile) return;
            containerRef.current.innerHTML = '';
            widgetIdRef.current = window.turnstile.render(containerRef.current, {
              sitekey: siteKey,
              callback: (token) => {
                publish({ turnstile_token: token });
                setStatus('verified');
              },
              'expired-callback': () => {
                publish(null);
                setStatus('ready');
              },
              'error-callback': () => {
                publish(null);
                setStatus('error');
              },
              theme: 'auto',
              size: 'flexible',
            });
            setStatus('ready');
          })
          .catch(() => {
            if (!cancelled) setStatus('error');
          });
        return () => {
          cancelled = true;
          if (window.turnstile && widgetIdRef.current) {
            try {
              window.turnstile.remove(widgetIdRef.current);
            } catch {
              /* ignore */
            }
          }
        };
      }

      if (kind === 'tencent') {
        setStatus('ready');
        return;
      }

      if (kind === 'aliyun') {
        const sceneId = settings?.aliyun_captcha_scene_id?.trim() || '';
        const prefix = settings?.aliyun_captcha_prefix?.trim() || '';
        const region = (settings?.aliyun_captcha_region as string | undefined) || 'cn';
        if (!sceneId || !prefix) {
          setStatus('error');
          return;
        }
        setStatus('loading');
        window.AliyunCaptchaConfig = { region, prefix };
        void loadAliyunCaptchaScript()
          .then(() => {
            if (cancelled || !window.initAliyunCaptcha) return;
            window.initAliyunCaptcha({
              SceneId: sceneId,
              prefix,
              mode: 'popup',
              element: `#${aliyunIds.current.element}`,
              button: `#${aliyunIds.current.button}`,
              captchaVerifyCallback: (param) => {
                aliyunCached.current = param;
                publish({ turnstile_token: param });
                setStatus('verified');
                return { captchaResult: true };
              },
              onBizResultCallback: () => {},
              getInstance: () => {},
              language: langZh ? 'cn' : 'en',
            });
            setStatus('ready');
          })
          .catch(() => {
            if (!cancelled) setStatus('error');
          });
      }

      return () => {
        cancelled = true;
      };
    }, [kind, settings, langZh, publish]);

    const ensureProof = React.useCallback(async (): Promise<Sub2ApiCaptchaProof | null> => {
      if (kind === 'none') return {};
      if (kind === 'turnstile') {
        if (proof?.turnstile_token) return proof;
        return null;
      }
      if (kind === 'tencent') {
        if (proof?.tencent_captcha_ticket?.trim() && proof?.tencent_captcha_randstr?.trim()) {
          return proof;
        }
        const appId = settings?.tencent_captcha_app_id?.trim() || '';
        if (!appId) return null;
        try {
          const result = await verifyTencentCaptcha({
            appId,
            region: settings?.tencent_captcha_region,
            langZh,
          });
          if (!result) return null;
          const next: Sub2ApiCaptchaProof = {
            tencent_captcha_ticket: result.ticket,
            tencent_captcha_randstr: result.randstr,
          };
          publish(next);
          setStatus('verified');
          return next;
        } catch {
          setStatus('error');
          return null;
        }
      }
      if (kind === 'aliyun') {
        if (aliyunCached.current) {
          return { turnstile_token: aliyunCached.current };
        }
        return new Promise((resolve) => {
          const btn = document.getElementById(aliyunIds.current.button);
          if (!btn) {
            setStatus('error');
            resolve(null);
            return;
          }
          const started = Date.now();
          const timer = window.setInterval(() => {
            if (aliyunCached.current) {
              window.clearInterval(timer);
              resolve({ turnstile_token: aliyunCached.current });
              return;
            }
            if (Date.now() - started > 60_000) {
              window.clearInterval(timer);
              resolve(null);
            }
          }, 200);
          btn.click();
        });
      }
      return null;
    }, [kind, proof, settings, langZh, publish]);

    React.useImperativeHandle(
      ref,
      () => ({
        ensureProof,
        reset,
        kind: () => kind,
      }),
      [ensureProof, reset, kind],
    );

    if (kind === 'none') return null;

    return (
      <div className={cn('space-y-2', className)} data-sub2api-captcha={kind}>
        {kind === 'turnstile' ? (
          <>
            {status === 'loading' ? (
              <p className="text-xs text-secondary">{labels.turnstileLoading}</p>
            ) : null}
            {status === 'error' ? (
              <p className="text-xs text-danger">{labels.turnstileFailed}</p>
            ) : null}
            <div ref={containerRef} className="min-h-[65px] w-full" />
          </>
        ) : null}

        {kind === 'tencent' ? (
          <div className="space-y-2" data-sub2api-captcha-tencent="">
            <p className="text-xs text-secondary">
              {status === 'verified'
                ? labels.actionVerified
                : status === 'error'
                  ? labels.actionFailed
                  : labels.actionReady}
            </p>
            {status !== 'verified' ? (
              <Button
                type="button"
                variant="outline"
                className="w-full"
                data-sub2api-captcha-tencent-verify=""
                disabled={status === 'loading'}
                onClick={() => {
                  void ensureProof();
                }}
              >
                {status === 'error' ? labels.actionFailed : labels.actionNeeded}
              </Button>
            ) : null}
          </div>
        ) : null}

        {kind === 'aliyun' ? (
          <div className="space-y-2">
            <div id={aliyunIds.current.element} />
            <Button
              id={aliyunIds.current.button}
              type="button"
              variant="outline"
              className="w-full"
              disabled={status === 'loading' || status === 'verified'}
            >
              {status === 'verified'
                ? labels.actionVerified
                : status === 'error'
                  ? labels.actionFailed
                  : status === 'loading'
                    ? labels.turnstileLoading
                    : labels.actionNeeded}
            </Button>
          </div>
        ) : null}
      </div>
    );
  },
);
