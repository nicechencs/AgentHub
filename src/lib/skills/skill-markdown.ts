/**
 * SKILL.md preview helpers.
 *
 * Real skill files almost always start with YAML frontmatter:
 *
 * ```md
 * ---
 * name: foo
 * description: A long blurb for agents…
 * ---
 *
 * # Foo
 * …
 * ```
 *
 * If we feed that raw string to a Markdown renderer, the opening/closing `---`
 * become horizontal rules and `name:` / `description:` lines look like broken
 * prose — the top of the Skills preview panel looks especially bad.
 *
 * Preview mode should strip frontmatter and optionally surface `description`
 * as a clean meta lead. Source mode keeps the raw file.
 */

export type SkillMarkdownParts = {
  /** Frontmatter `name`, if present. */
  name: string | null;
  /** Frontmatter `description` (block scalars collapsed to one line). */
  description: string | null;
  /** Markdown body after the closing `---` fence (or original when no FM). */
  body: string;
  /** True when a well-formed frontmatter block was removed from `body`. */
  hasFrontmatter: boolean;
};

function stripBom(s: string): string {
  return s.charCodeAt(0) === 0xfeff ? s.slice(1) : s;
}

function unquote(v: string): string {
  const t = v.trim();
  if (
    (t.startsWith('"') && t.endsWith('"') && t.length >= 2) ||
    (t.startsWith("'") && t.endsWith("'") && t.length >= 2)
  ) {
    return t.slice(1, -1);
  }
  return t;
}

/**
 * Split SKILL.md into frontmatter fields + body for GUI preview.
 * Conservative (no full YAML): matches the backend's simple rules enough for
 * name / description, including `|` / `>` block scalars.
 */
export function splitSkillMarkdown(raw: string): SkillMarkdownParts {
  const content = stripBom(raw ?? '');
  const empty: SkillMarkdownParts = {
    name: null,
    description: null,
    body: content,
    hasFrontmatter: false,
  };

  // Optional leading whitespace then opening fence on its own line.
  const open = content.match(/^(?:[ \t]*\r?\n)*---[ \t]*\r?\n/);
  if (!open) return empty;

  const afterOpen = content.slice(open[0].length);
  // Closing fence: first line that is exactly ---
  let closeIndex = -1;
  let lineStart = 0;
  for (let i = 0; i <= afterOpen.length; i++) {
    if (i === afterOpen.length || afterOpen[i] === '\n') {
      let line = afterOpen.slice(lineStart, i);
      if (line.endsWith('\r')) line = line.slice(0, -1);
      if (line === '---') {
        closeIndex = lineStart;
        break;
      }
      lineStart = i + 1;
    }
  }
  if (closeIndex < 0) return empty;

  const frontmatter = afterOpen.slice(0, closeIndex);
  // Skip closing fence line
  let bodyStart = closeIndex + 3;
  if (afterOpen[bodyStart] === '\r') bodyStart += 1;
  if (afterOpen[bodyStart] === '\n') bodyStart += 1;
  let body = afterOpen.slice(bodyStart);
  // Drop a single leading blank line after FM (common authoring style)
  body = body.replace(/^(?:\r?\n)+/, '');

  let name: string | null = null;
  let description: string | null = null;

  const lines = frontmatter.split(/\r?\n/);
  let idx = 0;
  while (idx < lines.length) {
    const rawLine = lines[idx] ?? '';
    const line = rawLine.trim();
    if (!line || line.startsWith('#')) {
      idx += 1;
      continue;
    }
    const colon = line.indexOf(':');
    if (colon < 0) {
      idx += 1;
      continue;
    }
    const key = line.slice(0, colon).trim();
    const valueRaw = line.slice(colon + 1).trim();
    const keyIndent = rawLine.length - rawLine.trimStart().length;

    if (key === 'name' && valueRaw && valueRaw !== '|' && valueRaw !== '>') {
      name = unquote(valueRaw) || null;
      idx += 1;
      continue;
    }

    if (key === 'description') {
      if (valueRaw === '|' || valueRaw === '>') {
        const block: string[] = [];
        idx += 1;
        while (idx < lines.length) {
          const bl = lines[idx] ?? '';
          if (bl.trim() === '') {
            // blank line inside block → keep as paragraph break (space)
            block.push('');
            idx += 1;
            continue;
          }
          const indent = bl.length - bl.trimStart().length;
          if (indent <= keyIndent && bl.trim().includes(':')) break;
          if (indent <= keyIndent && bl.trim() !== '') break;
          block.push(bl.trim());
          idx += 1;
        }
        const joined = block
          .join(' ')
          .replace(/\s+/g, ' ')
          .trim();
        description = joined || null;
        continue;
      }
      if (valueRaw) {
        description = unquote(valueRaw) || null;
      }
      idx += 1;
      continue;
    }

    idx += 1;
  }

  return {
    name,
    description,
    body: body.length ? body : content,
    hasFrontmatter: true,
  };
}

/** Body only — safe to feed MarkdownView in preview mode. */
export function skillMarkdownBody(raw: string): string {
  return splitSkillMarkdown(raw).body;
}
