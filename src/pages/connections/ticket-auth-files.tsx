/**
 * Login-detail cards for the files associated with an authorization.
 */
import * as React from 'react';
import { ConfigFileCard } from '@/components/shared/ConfigFileCard';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { openPathInFileManager } from '@/lib/api/skill';
import { getAgentLivePaths } from '@/lib/api/install';
import {
  resolveCredentialFilePath,
  type AgentLivePathSet,
  type CredentialFileView,
} from '@/lib/credential-files';
import type { AgentKey } from '@/lib/types';

export function TicketAuthFiles({
  agentId,
  files,
}: {
  agentId: AgentKey;
  files: readonly CredentialFileView[];
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [livePaths, setLivePaths] = React.useState<AgentLivePathSet | null>(null);
  const [opening, setOpening] = React.useState<string | null>(null);

  React.useEffect(() => {
    let cancelled = false;
    void getAgentLivePaths(agentId)
      .then((paths) => {
        if (!cancelled) setLivePaths(paths);
      })
      .catch(() => {
        if (!cancelled) setLivePaths(null);
      });
    return () => {
      cancelled = true;
    };
  }, [agentId]);

  if (files.length === 0) return null;

  const openFile = async (fileName: string) => {
    const target = resolveCredentialFilePath(fileName, livePaths, agentId);
    setOpening(fileName);
    try {
      const opened = await openPathInFileManager(target);
      toast({
        title: t('connections.list.openedAuthFile'),
        description: opened,
        variant: 'success',
      });
    } catch (error) {
      toast({
        title: t('connections.list.openAuthFileFailed'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setOpening(null);
    }
  };

  return (
    <ul className="flex min-w-0 flex-col gap-2">
      {files.map((file) => (
        <li key={file.name} className="min-w-0">
          <ConfigFileCard
            name={file.name}
            path={resolveCredentialFilePath(file.name, livePaths, agentId)}
            content={file.content}
            copyLabel={t('connections.list.copyFile')}
            opening={opening === file.name}
            onOpen={() => void openFile(file.name)}
          />
        </li>
      ))}
    </ul>
  );
}
