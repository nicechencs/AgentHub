import { useEffect, useState } from 'react';
import { RotateCcw, Trash2 } from 'lucide-react';
import { ConfigFileCard } from '@/components/shared/ConfigFileCard';
import { DetailRow } from '@/components/shared/DetailRow';
import { InspectSurface } from '@/components/layout/InspectSurface';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { useToast } from '@/components/ui/toast';
import { inspectBackup } from '@/lib/api/backup';
import { openPathInFileManager } from '@/lib/api/skill';
import type { TranslateFn } from '@/lib/i18n';
import { fmtBytes } from '@/lib/utils';
import type { BackupFact, BackupInspect, BackupMeta } from '@/lib/types';
import { backupCardIdentity, backupFileLabel, fmtAbsoluteI18n } from './backup-format';

function factLabel(key: string, t: TranslateFn): string {
  if (key === 'email') return t('settings.backups.factEmail');
  if (key === 'secretTail') return t('settings.backups.factSecret');
  if (key === 'endpoint') return t('settings.backups.factEndpoint');
  if (key === 'provider') return t('settings.backups.factProvider');
  if (key === 'model') return t('settings.backups.factModel');
  return key;
}

export function BackupDetailPanel({
  backup,
  kindLabel,
  busy,
  width,
  onClose,
  onRestore,
  onDelete,
}: {
  backup: BackupMeta;
  kindLabel: string;
  busy: boolean;
  width?: number;
  onClose: () => void;
  onRestore: () => void;
  onDelete: () => void;
}) {
  const { t, lang } = useI18n();
  const { toast } = useToast();
  const [inspect, setInspect] = useState<BackupInspect | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [opening, setOpening] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setInspect(null);
    setError(null);
    void inspectBackup(backup.id)
      .then((row) => {
        if (!cancelled) setInspect(row);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [backup.id]);

  const identity = inspect?.identity?.trim() || backupCardIdentity(backup);
  const facts: BackupFact[] = inspect?.facts ?? [];
  const files = inspect?.files ?? [];

  const openFile = async (path: string) => {
    if (!path) return;
    setOpening(path);
    try {
      const opened = await openPathInFileManager(path);
      toast({
        title: t('settings.backups.openedFile'),
        description: opened,
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: t('settings.backups.openFileFailed'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setOpening(null);
    }
  };

  return (
    <InspectSurface
      asPanel
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={t('settings.backups.detailTitle')}
      description={identity}
      showCancel={false}
      width={width}
      primary={(
        <Button type="button" size="sm" variant="outline" disabled={busy} onClick={onRestore}>
          <RotateCcw className="h-3.5 w-3.5" />
          {t('common.restore')}
        </Button>
      )}
      danger={(
        <Button type="button" size="sm" variant="dangerOutline" disabled={busy} onClick={onDelete}>
          <Trash2 className="h-3.5 w-3.5" />
          {t('common.delete')}
        </Button>
      )}
    >
      <div id={`backup-detail-${backup.id}`} data-backup-detail={backup.id} className="flex flex-col gap-3 text-xs">
        <div className="grid gap-1.5 text-secondary sm:grid-cols-2">
          <DetailRow label={t('settings.backups.kindLabel')} value={kindLabel} />
          <DetailRow
            label={t('settings.backups.createdAt')}
            value={fmtAbsoluteI18n(backup.createdAt, lang)}
          />
          <DetailRow label={t('settings.backups.sizeLabel')} value={fmtBytes(backup.sizeBytes)} />
          {facts.map((fact) => (
            <DetailRow
              key={`${fact.key}:${fact.value}`}
              label={factLabel(fact.key, t)}
              value={fact.value}
              mono={fact.key === 'secretTail'}
              copyable={fact.key === 'email' || fact.key === 'endpoint'}
            />
          ))}
        </div>

        {error ? <p className="text-meta text-danger">{error}</p> : null}

        <section className="space-y-1.5">
          <h3 className="text-sm font-medium">{t('settings.backups.filesTitle')}</h3>
          {files.length === 0 && !error ? (
            <p className="text-meta text-muted">{t('settings.backups.noFileList')}</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {files.map((file) => (
                <li key={`${file.name}:${file.path}`}>
                  <ConfigFileCard
                    name={file.name || backupFileLabel(file.source ?? '')}
                    path={file.source || file.path}
                    content={file.content}
                    emptyHint={t('settings.backups.noTextContent')}
                    copyLabel={t('settings.backups.copyFile')}
                    openLabel={t('agents.detail.openFolder')}
                    opening={opening === file.path}
                    onOpen={() => void openFile(file.path)}
                  />
                  {!file.content && (file.facts?.length ?? 0) > 0 ? (
                    <div className="mt-1.5 grid gap-1 px-1">
                      {file.facts!.map((fact) => (
                        <DetailRow
                          key={`${file.name}:${fact.key}`}
                          label={factLabel(fact.key, t)}
                          value={fact.value}
                          mono={fact.key === 'secretTail'}
                        />
                      ))}
                    </div>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </InspectSurface>
  );
}
