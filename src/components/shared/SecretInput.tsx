import * as React from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';

/**
 * 脱敏回显；点眼睛切换明文/遮蔽，无二次确认、无自动再遮蔽提示。
 * value 传入明文(或已脱敏串),默认显示为脱敏形式。
 */
export function SecretInput({
  value,
  onChange,
  placeholder,
}: {
  value: string;
  onChange?: (v: string) => void;
  placeholder?: string;
}) {
  const [revealed, setRevealed] = React.useState(false);

  const mask = (v: string) => {
    if (v.length <= 7) return '••••••';
    return `${v.slice(0, 3)}-••••${v.slice(-4)}`;
  };

  return (
    <div className="relative">
      <Input
        type="text"
        value={revealed ? value : value ? mask(value) : ''}
        placeholder={placeholder}
        readOnly={!revealed && !!value}
        onChange={(e) => {
          // 仅在明文可见时接受编辑,避免基于脱敏串增量改写
          if (revealed || !value) onChange?.(e.target.value);
        }}
        onFocus={() => {
          if (!revealed && value) {
            // 聚焦进入编辑:清空并显示明文输入框,由用户重新输入
            onChange?.('');
            setRevealed(true);
          }
        }}
        className="pr-9 font-mono"
      />
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="absolute right-0.5 top-0.5 h-7 w-7"
        onClick={() => setRevealed((v) => !v)}
        title={revealed ? '遮蔽' : '显示'}
      >
        {revealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
      </Button>
    </div>
  );
}
