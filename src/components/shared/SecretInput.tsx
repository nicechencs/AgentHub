import * as React from 'react';
import { Eye, EyeOff } from 'lucide-react';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';

/**
 * 密钥输入。隐藏时用 password 类型，值始终是明文，避免脱敏串吞字。
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

  return (
    <div
      className="relative"
      onPointerDown={(event) => event.stopPropagation()}
    >
      <Input
        type={revealed ? 'text' : 'password'}
        value={value}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        onChange={(event) => onChange?.(event.target.value)}
        className="pr-9 font-mono"
      />
      <Button
        type="button"
        variant="ghost"
        size="icon"
        className="absolute right-0.5 top-0.5 h-7 w-7"
        onMouseDown={(event) => event.preventDefault()}
        onClick={() => setRevealed((current) => !current)}
        title={revealed ? '遮蔽' : '显示'}
      >
        {revealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
      </Button>
    </div>
  );
}
