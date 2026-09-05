import type { MouseEvent as ReactMouseEvent } from 'react';
import { Hint, Tip } from '@/components/ui/tooltip';
import { shortPath } from './project-format';

export function ProjectPathLink({
  path,
  disabled,
  ariaLabel,
  onOpen,
}: {
  path: string;
  disabled?: boolean;
  ariaLabel?: string;
  onOpen?: (e: ReactMouseEvent) => void;
}) {
  const text = shortPath(path, 40);
  return (
    <span className="min-w-0 w-full">
      {onOpen ? (
        <Hint label={path}>
          <button
            type="button"
            className="block w-full min-w-0 truncate text-left font-mono text-meta text-accent underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:text-muted disabled:no-underline"
            disabled={disabled}
            aria-label={ariaLabel}
            onClick={(e) => {
              e.stopPropagation();
              onOpen(e);
            }}
          >
            {text}
          </button>
        </Hint>
      ) : (
        <Tip label={path} className="block min-w-0 w-full truncate font-mono text-meta text-muted">
          {text}
        </Tip>
      )}
    </span>
  );
}
