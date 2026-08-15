import { Card, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import { updateSettings } from '@/lib/api/settings';
import { invalidateSkills } from '@/lib/hooks/useSkills';
import type { AppSettings } from '@/lib/types';
import { applyTheme } from '@/lib/theme';
import { useTheme } from '@/components/shared/ThemeProvider';
import {
  generalSettingsSaveDescription,
  SKILL_MARKET_OPTIONS,
} from './settings-format';
import { SettingsRow } from './settings-shared';

export function GeneralPanel({
  settings,
  patch,
  setSettings,
  committedThemeRef,
  saving,
  setSaving,
}: {
  settings: AppSettings;
  patch: (p: Partial<AppSettings>) => void;
  setSettings: (s: AppSettings) => void;
  committedThemeRef: React.MutableRefObject<AppSettings['theme']>;
  saving: boolean;
  setSaving: (v: boolean) => void;
}) {
  const { toast } = useToast();
  const { setTheme } = useTheme();
  return (
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <SettingsRow label="语言" description="暂不提供切换">
                <span className="text-sm text-secondary">界面目前仅中文</span>
              </SettingsRow>
              <SettingsRow label="主题" description="浅色 / 深色 / 跟随系统">
                <Select
                  value={settings.theme}
                  onValueChange={(v) => {
                    const theme = v as AppSettings['theme'];
                    patch({ theme });
                    applyTheme(theme);
                  }}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="light">浅色</SelectItem>
                    <SelectItem value="dark">深色</SelectItem>
                    <SelectItem value="system">跟随系统</SelectItem>
                  </SelectContent>
                </Select>
              </SettingsRow>
              <SettingsRow
                label="开机自启"
                description="登录后启动"
                descriptionTip="写入操作系统登录项（Windows 启动项 / macOS Login Item）。保存「外观与行为」后生效。"
              >
                <Switch
                  checked={settings.autoStart}
                  onCheckedChange={(v) => patch({ autoStart: v })}
                />
              </SettingsRow>
              <SettingsRow
                label="关闭到托盘"
                description="关窗不退出"
                descriptionTip="点击关闭按钮后隐藏到系统托盘，进程保持运行。Windows 可从托盘图标恢复；macOS 可从菜单栏托盘或 Dock 图标恢复。"
              >
                <Switch
                  checked={settings.closeToTray}
                  onCheckedChange={(v) => patch({ closeToTray: v })}
                />
              </SettingsRow>
              <SettingsRow
                label="技能市场"
                description="远程技能源"
                descriptionTip="自动：优先 skills.sh，网络不可达时回退 skillhub.cn。也可固定只用其一。"
              >
                <Select
                  value={settings.skillMarketSource ?? 'auto'}
                  onValueChange={(v) =>
                    patch({ skillMarketSource: v as AppSettings['skillMarketSource'] })
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {SKILL_MARKET_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </SettingsRow>
            </CardContent>
            <CardFooter>
              <Button
                disabled={saving}
                onClick={() => {
                  void (async () => {
                    setSaving(true);
                    try {
                      const next = await updateSettings({
                        theme: settings.theme,
                        autoStart: settings.autoStart,
                        closeToTray: settings.closeToTray,
                        skillMarketSource: settings.skillMarketSource ?? 'auto',
                      });
                      setSettings(next);
                      committedThemeRef.current = next.theme;
                      setTheme(next.theme);
                      // 必须在保存成功后再清市场缓存，否则会继续展示上一源
                      invalidateSkills('market');
                      toast({
                        title: '常规设置已保存',
                        description: generalSettingsSaveDescription(next),
                        variant: 'success',
                      });
                    } catch (e) {
                      toast({
                        title: '保存失败',
                        description: String(e),
                        variant: 'danger',
                      });
                    } finally {
                      setSaving(false);
                    }
                  })();
                }}
              >
                {saving ? '保存中…' : '保存'}
              </Button>
            </CardFooter>
          </Card>
  );
}
