import { EnterKeyMark, ShortcutKbd } from '@/components/ui/shortcut-kbd';
import { useI18n } from '@/components/shared/LanguageProvider';
import { detectHostPlatform } from '@/lib/platform-detect';
import { CHAT_SHORTCUT_ROWS, chatShortcutChord } from './chat-shortcuts';

function ShortcutKeys({ keys }: { keys: string }) {
  if (keys === 'Enter') return <EnterKeyMark />;
  if (keys === 'Shift+Enter') {
    return (
      <span className="inline-flex items-center gap-0.5">
        <ShortcutKbd>⇧</ShortcutKbd>
        <EnterKeyMark />
      </span>
    );
  }
  return <ShortcutKbd>{keys}</ShortcutKbd>;
}

export function ChatShortcutOverview({ className }: { className?: string }) {
  const { t } = useI18n();
  const platform = detectHostPlatform();

  return (
    <ul className={className ?? 'space-y-2'}>
      {CHAT_SHORTCUT_ROWS.map((row) => (
        <li key={row.id} className="flex items-center justify-between gap-3">
          <span className="text-body text-primary">{t(row.actionKey)}</span>
          <ShortcutKeys keys={chatShortcutChord(row.keys, platform)} />
        </li>
      ))}
    </ul>
  );
}
