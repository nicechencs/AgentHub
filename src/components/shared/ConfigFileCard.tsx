import * as React from 'react';
import { Check, Copy, FolderOpen } from 'lucide-react';
import { Button } from '@/components/ui/button';

/** Filename + path, copy, and 「目录」 — used by login details and backup details. */
export function ConfigFileCard({
  name,
  path,
  content,
  emptyHint,
  copyLabel,
  openLabel,
  opening = false,
  onOpen,
}: {
  name: string;
  path: string;
  content?: string | null;
  emptyHint?: string;
  copyLabel: string;
  openLabel: string;
  opening?: boolean;
  onOpen: () => void;
}) {
  const [copied, setCopied] = React.useState(false);
  const text = content?.trim() ? content : null;
  const onCopy = () => {
    if (!text) return;
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    }).catch(() => {});
  };

  return (
    <div className="rounded-card border border-border bg-subtle/60">
      <div className="flex items-start justify-between gap-2 px-3 py-2">
        <div className="min-w-0">
          <p className="truncate font-mono text-sm font-medium text-secondary">{name}</p>
          {path ? (
            <p className="truncate font-mono text-meta text-muted">{path}</p>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {text ? (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 shrink-0 px-2"
              title={copyLabel}
              aria-label={copyLabel}
              onClick={onCopy}
            >
              {copied ? <Check className="h-3 w-3 text-success" /> : <Copy className="h-3 w-3" />}
              {copyLabel}
            </Button>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="ghost"
            className="h-7 shrink-0 px-2"
            disabled={opening || !path}
            title={path || openLabel}
            aria-label={openLabel}
            onClick={onOpen}
          >
            <FolderOpen className="h-3 w-3" />
            {openLabel}
          </Button>
        </div>
      </div>
      {text ? (
        <pre className="max-h-64 overflow-auto border-t border-border px-3 py-2 font-mono text-meta whitespace-pre-wrap break-all text-secondary">
          {text}
        </pre>
      ) : emptyHint ? (
        <p className="border-t border-border px-3 py-2 text-meta text-muted">{emptyHint}</p>
      ) : null}
    </div>
  );
}
