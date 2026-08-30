import { SourcePreview } from '@/components/shared/SourcePreview';

/**
 * JSON/TOML editor used by the supplier advanced config.
 * Highlight/fold chrome lives in SourcePreview; this wrapper stays editable.
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
  return (
    <SourcePreview
      value={value}
      format={format}
      readOnly={readOnly}
      pretty={false}
      density="editor"
      onChange={onChange}
    />
  );
}
