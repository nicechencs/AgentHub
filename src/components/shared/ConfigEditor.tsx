import CodeMirror from '@uiw/react-codemirror';
import { json } from '@codemirror/lang-json';
import { StreamLanguage } from '@codemirror/language';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { useTheme } from '@/components/shared/ThemeProvider';
import { resolveTheme } from '@/lib/theme';

/**
 * CodeMirror 封装:JSON/TOML 高亮(docs/ui-design.md §5 ConfigEditor)。
 * 敏感键脱敏由 mock 数据层保证(configText 中的密钥已是 sk-•••• 形式);
 * 真实实现由 core 的 SensitiveCredentialKeys 模式在 DTO 出 core 前统一脱敏。
 */
export function ConfigEditor({
  value,
  onChange,
  format,
  readOnly = false,
}: {
  value: string;
  onChange?: (v: string) => void;
  format: 'json' | 'toml';
  readOnly?: boolean;
}) {
  const { theme } = useTheme();
  const cmTheme = resolveTheme(theme) === 'dark' ? 'dark' : 'light';

  return (
    <div className="max-h-80 min-h-24 overflow-auto rounded-card border border-border [&_.cm-editor]:min-h-24 [&_.cm-editor]:bg-canvas [&_.cm-editor]:text-xs [&_.cm-gutters]:bg-canvas [&_.cm-gutters]:text-muted [&_.cm-activeLine]:bg-hover">
      <CodeMirror
        value={value}
        height="auto"
        minHeight="96px"
        theme={cmTheme}
        readOnly={readOnly}
        extensions={[format === 'json' ? json() : StreamLanguage.define(toml)]}
        onChange={(v) => onChange?.(v)}
        basicSetup={{ lineNumbers: true, foldGutter: false, highlightActiveLine: true }}
      />
    </div>
  );
}
