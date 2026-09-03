export type SessionRecordLine = {
  speaker: string;
  text: string;
};

/** Plain-text session record: speaker, then body, blank line between turns. */
export function formatSessionRecordText(lines: SessionRecordLine[]): string {
  return lines
    .map((line) => {
      const text = line.text.replace(/\r\n/g, '\n').trim();
      if (!text) return '';
      const speaker = line.speaker.trim();
      return speaker ? `${speaker}\n${text}` : text;
    })
    .filter(Boolean)
    .join('\n\n');
}
