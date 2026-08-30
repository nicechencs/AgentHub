import { cn } from '@/lib/utils';

/** Product mark path (`public/app-icon.png`), respects Vite `base: './'`. */
export function appIconUrl(): string {
  const base = import.meta.env.BASE_URL || './';
  return `${base.endsWith('/') ? base : `${base}/`}app-icon.png`;
}

/**
 * In-app product mark. Tile fill follows `--accent` via `currentColor`
 * (live CSS, not a one-shot SVG presentation attribute).
 * Desktop / installer icons stay the default indigo PNG.
 */
export function AppLogo({
  className,
  size = 24,
  alt = '',
}: {
  className?: string;
  size?: number;
  alt?: string;
}) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      role={alt ? 'img' : 'presentation'}
      aria-label={alt || undefined}
      aria-hidden={alt ? undefined : true}
      data-app-logo=""
      className={cn(
        'pointer-events-none select-none overflow-hidden rounded-mark text-accent',
        className,
      )}
      style={{ color: 'var(--accent)' }}
    >
      <rect width="1024" height="1024" rx="224" fill="currentColor" />
      <path
        d="M300 682 430 342h164l130 340M354 548h316"
        fill="none"
        stroke="#fff"
        strokeWidth="78"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="300" cy="682" r="52" fill="#fff" />
      <circle cx="512" cy="548" r="52" fill="#fff" />
      <circle cx="724" cy="682" r="52" fill="#fff" />
    </svg>
  );
}
