import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { SettingsRow } from './settings-shared';

export function SecurityPanel() {
  return (
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <SettingsRow
                label="凭据展示"
                description="界面不回显明文"
                descriptionTip="密钥与令牌经 SecretInput 管理；默认脱敏显示，可点眼睛切换明文。"
              >
                <Badge variant="success">不回显</Badge>
              </SettingsRow>
              <SettingsRow
                label="存储方式"
                description="本地数据目录存储，界面脱敏展示"
                descriptionTip="凭据写入本机数据目录；界面默认脱敏，当前不提供 keyring 或落盘加密。"
              >
                <span className="text-sm text-secondary">本地数据目录</span>
              </SettingsRow>
            </CardContent>
          </Card>
  );
}
