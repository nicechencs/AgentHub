/**
 * AgentHub update mirror Worker.
 *
 * Serves only latest.json, known AgentHub_* installers, and matching *.sig
 * from R2 binding UPDATES_BUCKET. Everything else returns 404.
 * Signing private keys must NEVER be stored in R2 or Worker env.
 */

export interface Env {
  UPDATES_BUCKET: R2Bucket;
}

const FEED_NAME = 'latest.json';

const INSTALLER_RE =
  /^AgentHub_[A-Za-z0-9._+-]+\.(exe|msi|dmg|deb|AppImage|app\.tar\.gz)$/;

const SIG_RE =
  /^AgentHub_[A-Za-z0-9._+-]+\.(exe|msi|dmg|deb|AppImage|app\.tar\.gz)\.sig$/;

function normalizeKey(pathname: string): string | null {
  const raw = pathname.replace(/^\/+/, '');
  if (!raw || raw.includes('/') || raw.includes('\\') || raw.includes('..')) {
    return null;
  }
  try {
    return decodeURIComponent(raw);
  } catch {
    return null;
  }
}

function isAllowedKey(key: string): boolean {
  return key === FEED_NAME || SIG_RE.test(key) || INSTALLER_RE.test(key);
}

function cacheControlFor(key: string): string {
  if (key === FEED_NAME) {
    return 'public, max-age=60, s-maxage=60, stale-while-revalidate=300';
  }
  if (key.endsWith('.sig')) {
    return 'public, max-age=3600, s-maxage=86400, immutable';
  }
  return 'public, max-age=86400, s-maxage=604800, immutable';
}

function contentTypeFor(key: string): string {
  if (key === FEED_NAME || key.endsWith('.sig')) {
    return 'application/json; charset=utf-8';
  }
  if (key.endsWith('.app.tar.gz')) return 'application/gzip';
  return 'application/octet-stream';
}

function notFound(): Response {
  return new Response('Not Found', {
    status: 404,
    headers: {
      'cache-control': 'no-store',
      'content-type': 'text/plain; charset=utf-8',
      'x-content-type-options': 'nosniff',
    },
  });
}

function methodNotAllowed(): Response {
  return new Response('Method Not Allowed', {
    status: 405,
    headers: {
      allow: 'GET, HEAD',
      'cache-control': 'no-store',
      'content-type': 'text/plain; charset=utf-8',
    },
  });
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);

    if (request.method !== 'GET' && request.method !== 'HEAD') {
      return methodNotAllowed();
    }

    if (url.pathname === '/' || url.pathname === '') {
      return notFound();
    }

    const key = normalizeKey(url.pathname);
    if (!key || !isAllowedKey(key)) {
      return notFound();
    }

    const object = await env.UPDATES_BUCKET.get(key);
    if (!object) {
      return notFound();
    }

    const headers = new Headers();
    object.writeHttpMetadata(headers);
    headers.set('etag', object.httpEtag);
    headers.set('cache-control', cacheControlFor(key));
    headers.set('content-type', contentTypeFor(key));
    headers.set('x-content-type-options', 'nosniff');
    headers.set('access-control-allow-origin', '*');
    headers.set('access-control-allow-methods', 'GET, HEAD');

    if (request.method === 'HEAD') {
      return new Response(null, { status: 200, headers });
    }

    return new Response(object.body, { status: 200, headers });
  },
};
