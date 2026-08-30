import * as React from 'react';
import { Check, Copy } from 'lucide-react';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { SourcePreview } from '@/components/shared/SourcePreview';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';

/** Filename + path, copy, and 「目录」 — used by login details and backup details. */
export function ConfigFileCard({
  name,
  path,
  content,
  emptyHint,
  copyLabel,
  opening = false,
  onOpen,
}: {
  name: string;
  path: string;
  content?: string | null;
  emptyHint?: string;
  copyLabel: string;
  opening?: boolean;
  onOpen: () => void;
}) {
  const { t } = useI18n();
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
    <div className="overflow-hidden rounded-card border border-border bg-subtle/60">
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
          <OpenDirButton
            labeled
            disabled={opening || !path}
            title={path || t('common.directory')}
            onClick={onOpen}
          />
        </div>
      </div>
      {text ? (
        <SourcePreview
          value={text}
          fileName={name}
          className="rounded-none border-x-0 border-b-0"
        />
      ) : emptyHint ? (
        <p className="border-t border-border px-3 py-2 text-meta text-muted">{emptyHint}</p>
      ) : null}
    </div>
  );
}
