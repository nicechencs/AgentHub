import type { MouseEvent as ReactMouseEvent } from 'react';
import { Hint } from '@/components/ui/tooltip';
import { shortPath } from './project-format';

export function ProjectPathLink({
  path,
  disabled,
  ariaLabel,
  onOpen,
}: {
  path: string;
  disabled?: boolean;
  ariaLabel: string;
  onOpen: (e: ReactMouseEvent) => void;
}) {
  return (
    <span className="min-w-0 flex-1">
      <Hint label={path}>
        <button
          type="button"
          className="max-w-full truncate text-left font-mono text-meta text-accent underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:text-muted disabled:no-underline"
          disabled={disabled}
          aria-label={ariaLabel}
          onClick={(e) => {
            e.stopPropagation();
            onOpen(e);
          }}
        >
          {shortPath(path, 40)}
        </button>
      </Hint>
    </span>
  );
}
