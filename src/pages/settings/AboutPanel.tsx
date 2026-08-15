import { ExternalLink } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { useToast } from '@/components/ui/toast';
import type { UpdateInfo } from '@/lib/api/update';
import { openExternalLink } from '@/lib/open-external';
import type { AppSettings } from '@/lib/types';
import { GITHUB_REPO_URL } from './settings-format';
import { SettingsRow } from './settings-shared';

export function AboutPanel({
  settings,
  pendingUpdate,
  checking,
  installing,
  checkUpdate,
  installUpdate,
}: {
  settings: AppSettings;
  pendingUpdate: UpdateInfo | null;
  checking: boolean;
  installing: boolean;
  checkUpdate: () => void;
  installUpdate: () => void;
}) {
  const { toast } = useToast();
  return (
    <>
      <Card>
        <CardContent className="divide-y divide-border pt-1">
          <SettingsRow
            label="版本"
            description={
              pendingUpdate
                ? `可更新至 v${pendingUpdate.version}`
                : '检查发布频道的新版本'
            }
          >
            <span className="flex items-center gap-2 font-mono text-sm text-secondary">
              v{settings.appVersion}
              {pendingUpdate && <Badge variant="warning">有新版本</Badge>}
            </span>
            <Button
              size="sm"
              variant="outline"
              disabled={checking || installing}
              onClick={checkUpdate}
            >
              {checking ? '检查中…' : '检查更新'}
            </Button>
          </SettingsRow>
          {pendingUpdate && (
            <SettingsRow
              label="新版本"
              description={pendingUpdate.notes?.split('\n')[0] || '签名安装包一键升级'}
              descriptionTip={pendingUpdate.notes || undefined}
            >
              <span className="font-mono text-sm text-accent">
                v{pendingUpdate.version}
              </span>
              <Button size="sm" disabled={installing || checking} onClick={installUpdate}>
                {installing ? '更新中…' : '一键更新'}
              </Button>
            </SettingsRow>
          )}
          <SettingsRow
            label="GitHub"
            description="源码、Issue 与发布页"
            descriptionTip={GITHUB_REPO_URL}
          >
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                void openExternalLink(GITHUB_REPO_URL).catch((e) => {
                  toast({
                    title: '无法打开 GitHub',
                    description: e instanceof Error ? e.message : String(e),
                    variant: 'danger',
                  });
                });
              }}
            >
              <ExternalLink className="mr-1.5 h-3.5 w-3.5" />
              打开仓库
            </Button>
          </SettingsRow>
        </CardContent>
      </Card>
      <p className="mt-3 text-xs text-muted">
        AgentHub — 统一管理 AI coding agent 的配置、账号与用量。
      </p>
    </>
  );
}
