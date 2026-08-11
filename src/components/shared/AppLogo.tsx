import { cn } from '@/lib/utils';

/** Product mark path (`public/app-icon.png`), respects Vite `base: './'`. */
export function appIconUrl(): string {
  const base = import.meta.env.BASE_URL || './';
  return `${base.endsWith('/') ? base : `${base}/`}app-icon.png`;
}

/** AgentHub product logo (matches desktop / installer icon). */
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
    <img
      src={appIconUrl()}
      width={size}
      height={size}
      alt={alt}
      draggable={false}
      className={cn('pointer-events-none select-none object-cover', className)}
    />
  );
}
