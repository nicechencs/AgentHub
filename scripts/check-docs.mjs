import { readdirSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { dirname, extname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const rootDir = resolve(scriptDir, '..');
const skipDirectories = new Set([
  '.git',
  '.codegraph',
  'node_modules',
  'target',
  'dist',
  'release-out',
  'temp',
  '.tmp-usage-compare',
]);

const allowedTypes = new Set([
  'navigation',
  'tutorial',
  'guide',
  'how-to',
  'getting-started',
  'concept',
  'reference',
  'architecture',
  'integration',
  'operations',
  'ui',
  'explanation',
  'decision',
  'proposal',
  'status',
  'governance',
  'archive',
]);

const allowedStatuses = new Set(['current', 'proposed', 'historical', 'archived']);

const typeAliases = new Map([
  ['domain-concept', 'concept'],
  ['decision-index', 'decision'],
  ['product-boundary', 'decision'],
]);

const statusAliases = new Map([
  ['current-contract', 'current'],
  ['current-contract-with-a-future-migration-direction', 'current'],
  ['current-index', 'current'],
  ['current-decision', 'current'],
]);

const legacyMarker = /历史|旧路径|旧入口|旧名称|过时|兼容|重定向|已废弃|legacy|deprecated|redirect|historical/i;

function normalizeEnum(value) {
  return value.trim().toLowerCase().replace(/\s+/g, '-');
}

function normalizeType(value) {
  const normalized = normalizeEnum(value);
  return typeAliases.get(normalized) ?? normalized;
}

function normalizeStatus(value) {
  const normalized = normalizeEnum(value);
  return statusAliases.get(normalized) ?? normalized;
}

function displayPath(filePath) {
  return relative(rootDir, filePath).replaceAll('\\', '/');
}

function isWithinRoot(candidate, projectRoot) {
  const boundary = resolve(projectRoot);
  const candidateRelative = relative(boundary, candidate);
  return candidateRelative === ''
    || (candidateRelative !== '..'
      && !candidateRelative.startsWith(`..${sep}`)
      && !isAbsolute(candidateRelative));
}

function isAbsoluteLinkPath(pathPart) {
  return isAbsolute(pathPart)
    || pathPart.startsWith('/')
    || pathPart.startsWith('\\')
    || /^[A-Za-z]:[\\/]/.test(pathPart)
    || /^file:/i.test(pathPart);
}

function splitLinkTarget(target) {
  const hashIndex = target.indexOf('#');
  const pathPart = hashIndex === -1 ? target : target.slice(0, hashIndex);
  const rawFragment = hashIndex === -1 ? '' : target.slice(hashIndex + 1);
  const cleanPath = pathPart.split('?')[0];
  let fragment = '';
  let path = cleanPath;
  try {
    path = decodeURIComponent(cleanPath);
    fragment = decodeURIComponent(rawFragment);
  } catch {
    return { error: 'invalid-encoding' };
  }
  return { path, fragment };
}

/** Resolve a Markdown target lexically and through realpath when it exists. */
function resolveLinkTarget(target, sourceFile, projectRoot = rootDir) {
  const trimmed = target.trim();
  if (!trimmed || /^(?:https?:|mailto:|tel:|ftp:|data:)/i.test(trimmed)) {
    return { kind: 'external' };
  }

  const split = splitLinkTarget(trimmed);
  if (split.error) {
    return { kind: 'error', code: split.error, message: 'link contains invalid URL encoding' };
  }
  if (isAbsoluteLinkPath(split.path)) {
    return { kind: 'error', code: 'absolute-path', message: 'absolute local paths are not allowed' };
  }

  const destination = split.path
    ? resolve(dirname(sourceFile), split.path)
    : resolve(sourceFile);
  if (!isWithinRoot(destination, projectRoot)) {
    return { kind: 'error', code: 'outside-root', message: 'link resolves outside the repository root' };
  }

  let realDestination = null;
  try {
    realDestination = realpathSync(destination);
  } catch (error) {
    if (error?.code !== 'ENOENT') {
      return {
        kind: 'error',
        code: 'realpath-failed',
        message: `unable to resolve link target realpath: ${error?.code ?? error?.message ?? String(error)}`,
      };
    }
  }
  if (realDestination && !isWithinRoot(realDestination, projectRoot)) {
    return { kind: 'error', code: 'realpath-outside-root', message: 'link realpath escapes the repository root' };
  }
  return { kind: 'local', path: destination, realPath: realDestination, fragment: split.fragment };
}

function addError(errors, filePath, lineNumber, message) {
  const location = lineNumber ? `${displayPath(filePath)}:${lineNumber}` : displayPath(filePath);
  errors.push(`${location}: ${message}`);
}

function collectMarkdown(directory, errors = []) {
  const files = [];
  let entries;
  try {
    entries = readdirSync(directory, { withFileTypes: true });
  } catch (error) {
    const code = error?.code ?? error?.message ?? String(error);
    addError(errors, directory, 1, `unable to read Markdown directory (${code})`);
    return files;
  }
  for (const entry of entries) {
    if (entry.isDirectory()) {
      if (!skipDirectories.has(entry.name)) {
        files.push(...collectMarkdown(join(directory, entry.name), errors));
      }
      continue;
    }
    if (entry.isFile() && extname(entry.name).toLowerCase() === '.md') {
      files.push(join(directory, entry.name));
    }
  }
  return files.sort();
}

function isArchive(filePath) {
  const relativePath = displayPath(filePath);
  return relativePath === 'docs/archive' || relativePath.startsWith('docs/archive/');
}

function isDocsFile(filePath) {
  return displayPath(filePath).startsWith('docs/');
}

function parseFrontMatter(filePath, lines, errors) {
  const firstContentLine = lines.findIndex((line) => line.trim() !== '');
  if (firstContentLine === -1 || lines[firstContentLine].trim() !== '---') {
    return null;
  }

  const end = lines.findIndex((line, index) => index > firstContentLine && line.trim() === '---');
  if (end === -1) {
    addError(errors, filePath, firstContentLine + 1, 'front matter starts with --- but has no closing ---');
    return null;
  }

  const metadata = new Map();
  for (let index = firstContentLine + 1; index < end; index += 1) {
    const match = lines[index].match(/^([A-Za-z][A-Za-z0-9_-]*):\s*(.*?)\s*$/);
    if (match) {
      metadata.set(match[1], match[2]);
    }
  }
  return { metadata, end };
}

function parseMetadata(filePath, lines, errors) {
  const frontMatter = parseFrontMatter(filePath, lines, errors);
  if (frontMatter) {
    return frontMatter.metadata;
  }

  // Existing organized pages use a short, human-readable blockquote header.
  const metadata = new Map();
  for (const line of lines.slice(0, 30)) {
    const match = line.match(/^>\s*(Status|Type|Last verified):\s*(.*?)\s*$/i);
    if (!match) {
      continue;
    }
    const key = match[1].toLowerCase() === 'last verified' ? 'updated' : match[1].toLowerCase();
    metadata.set(key, match[2]);
  }
  const title = lines.find((line) => /^\s*#\s+\S/.test(line));
  if (title) {
    metadata.set('title', title.replace(/^\s*#\s+/, '').trim());
  }
  return metadata.size > 0 ? metadata : null;
}

function headingSlug(value) {
  return value
    .replace(/<[^>]*>/g, '')
    .replace(/!\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/[`*~]/g, '')
    .trim()
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s_-]/gu, '')
    .replace(/[\s-]+/g, '-')
    .replace(/^-+|-+$/g, '');
}

function openingFence(line) {
  const match = line.match(/^ {0,3}([`~]{3,})(.*)$/);
  if (!match) {
    return null;
  }
  const marker = match[1][0];
  if ([...match[1]].some((character) => character !== marker)) {
    return null;
  }
  if (marker === '`' && match[2].includes('`')) {
    return null;
  }
  return { marker, length: match[1].length };
}

function closesFence(line, fence) {
  const match = line.match(/^ {0,3}([`~]{3,})[ \t]*$/);
  return Boolean(match)
    && match[1][0] === fence.marker
    && match[1].length >= fence.length
    && [...match[1]].every((character) => character === fence.marker);
}

function forEachMarkdownLine(lines, callback) {
  let fence = null;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (fence) {
      if (closesFence(line, fence)) {
        fence = null;
      }
      continue;
    }
    const opening = openingFence(line);
    if (opening) {
      fence = opening;
      continue;
    }
    // Four-space/tab indented blocks are code blocks, not Markdown prose.
    if (/^(?: {4}|\t)/.test(line)) {
      continue;
    }
    callback(line, index);
  }
}

function maskInlineCode(line) {
  // Keep JavaScript string indexes in UTF-16 code units, matching the scanners below.
  const characters = line.split('');
  let index = 0;
  while (index < characters.length) {
    if (characters[index] !== '`' || (index > 0 && characters[index - 1] === '\\')) {
      index += 1;
      continue;
    }
    let length = 1;
    while (characters[index + length] === '`') {
      length += 1;
    }
    let close = index + length;
    while (close < characters.length) {
      if (characters[close] === '`' && (close === 0 || characters[close - 1] !== '\\')) {
        let closeLength = 1;
        while (characters[close + closeLength] === '`') {
          closeLength += 1;
        }
        if (closeLength === length) {
          for (let position = index; position < close + length; position += 1) {
            characters[position] = ' ';
          }
          index = close + length;
          close = -1;
          break;
        }
        close += closeLength;
      } else {
        close += 1;
      }
    }
    if (close !== -1) {
      // An unmatched backtick run is literal Markdown, so leave it visible to the link scanner.
      index += length;
    }
  }
  return characters.join('');
}

function findUnescaped(line, character, start) {
  for (let index = start; index < line.length; index += 1) {
    if (line[index] === '\\') {
      index += 1;
      continue;
    }
    if (line[index] === character) {
      return index;
    }
  }
  return -1;
}

function scanBareDestination(line, start, stopOnWhitespace) {
  let depth = 0;
  for (let index = start; index < line.length; index += 1) {
    const character = line[index];
    if (character === '\\') {
      index += 1;
      continue;
    }
    if (character === '(') {
      depth += 1;
      continue;
    }
    if (character === ')') {
      if (depth === 0) {
        return { target: line.slice(start, index), end: index };
      }
      depth -= 1;
      continue;
    }
    if (stopOnWhitespace && depth === 0 && /\s/.test(character)) {
      return { target: line.slice(start, index), end: index };
    }
  }
  return { target: line.slice(start), end: line.length, unterminated: !stopOnWhitespace || depth !== 0 };
}

function findClosingParenthesis(line, start) {
  let depth = 0;
  for (let index = start; index < line.length; index += 1) {
    const character = line[index];
    if (character === '\\') {
      index += 1;
      continue;
    }
    if (character === '(') {
      depth += 1;
    } else if (character === ')') {
      if (depth === 0) {
        return index;
      }
      depth -= 1;
    }
  }
  return -1;
}

function parseInlineDestination(line, openIndex) {
  let start = openIndex + 1;
  while (/\s/.test(line[start] ?? '')) {
    start += 1;
  }
  if (line[start] === '<') {
    const closeAngle = findUnescaped(line, '>', start + 1);
    if (closeAngle === -1) {
      return null;
    }
    const closeParenthesis = findClosingParenthesis(line, closeAngle + 1);
    return closeParenthesis === -1
      ? null
      : { target: line.slice(start + 1, closeAngle), end: closeParenthesis };
  }

  const bare = scanBareDestination(line, start, true);
  if (bare.end === line.length && bare.unterminated) {
    return null;
  }
  const closeParenthesis = bare.end < line.length && line[bare.end] === ')'
    ? bare.end
    : findClosingParenthesis(line, bare.end);
  return closeParenthesis === -1 ? null : { target: bare.target, end: closeParenthesis };
}

function parseReferenceDestination(line, start) {
  while (/\s/.test(line[start] ?? '')) {
    start += 1;
  }
  if (line[start] === '<') {
    const closeAngle = findUnescaped(line, '>', start + 1);
    return closeAngle === -1 ? null : { target: line.slice(start + 1, closeAngle) };
  }
  return scanBareDestination(line, start, true);
}

function parseHeadings(lines) {
  const headings = [];
  forEachMarkdownLine(lines, (line, index) => {
    const match = line.match(/^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$/);
    if (match) {
      headings.push({ lineNumber: index + 1, text: match[1], slug: headingSlug(match[1]) });
    }
  });
  return headings;
}

function findDuplicateHeadingAnchors(lines) {
  const seen = new Map();
  const duplicates = [];
  for (const heading of parseHeadings(lines)) {
    const previous = seen.get(heading.slug);
    if (previous) {
      duplicates.push({ slug: heading.slug, firstLine: previous, lineNumber: heading.lineNumber });
    } else if (heading.slug) {
      seen.set(heading.slug, heading.lineNumber);
    }
  }
  return duplicates;
}

function parseLinks(lines) {
  const links = [];
  forEachMarkdownLine(lines, (originalLine, index) => {
    const line = maskInlineCode(originalLine);
    for (let cursor = 0; cursor < line.length; cursor += 1) {
      if (line[cursor] !== '[' || (cursor > 0 && line[cursor - 1] === '\\')) {
        continue;
      }
      const labelEnd = findUnescaped(line, ']', cursor + 1);
      if (labelEnd === -1 || line[labelEnd + 1] !== '(') {
        continue;
      }
      const destination = parseInlineDestination(line, labelEnd + 1);
      if (!destination) {
        continue;
      }
      links.push({ lineNumber: index + 1, target: destination.target.trim() });
      cursor = destination.end;
    }

    const definition = line.match(/^ {0,3}\[[^\]]+\]:[ \t]*/);
    if (definition) {
      const destination = parseReferenceDestination(line, definition[0].length);
      if (destination?.target) {
        links.push({ lineNumber: index + 1, target: destination.target.trim() });
      }
    }
  });
  return links;
}

function safeReadFile(filePath, sourceFile, lineNumber, target, errors) {
  try {
    return readFileSync(filePath, 'utf8');
  } catch (error) {
    const code = error?.code ?? error?.message ?? String(error);
    const reason = error?.code === 'ENOENT' ? 'local link target does not exist' : `unable to read local link target (${code})`;
    addError(errors, sourceFile, lineNumber, `${reason}: ${target}`);
    return null;
  }
}

function safeStat(filePath, sourceFile, lineNumber, target, errors) {
  try {
    return statSync(filePath);
  } catch (error) {
    const code = error?.code ?? error?.message ?? String(error);
    const reason = error?.code === 'ENOENT' ? 'local link target does not exist' : `unable to inspect local link target (${code})`;
    addError(errors, sourceFile, lineNumber, `${reason}: ${target}`);
    return null;
  }
}

function statIfPresent(filePath) {
  try {
    return statSync(filePath);
  } catch {
    return null;
  }
}

function safeRealpath(filePath, sourceFile, lineNumber, target, projectRoot, errors) {
  try {
    const realPath = realpathSync(filePath);
    if (!isWithinRoot(realPath, projectRoot)) {
      addError(errors, sourceFile, lineNumber, `link realpath escapes the repository root: ${target}`);
      return null;
    }
    return realPath;
  } catch (error) {
    if (error?.code !== 'ENOENT') {
      const code = error?.code ?? error?.message ?? String(error);
      addError(errors, sourceFile, lineNumber, `unable to resolve local link target (${code}): ${target}`);
    }
    return null;
  }
}

function readHeadings(filePath, sourceFile, lineNumber, target, errors) {
  const content = safeReadFile(filePath, sourceFile, lineNumber, target, errors);
  return content === null ? null : parseHeadings(content.split(/\r?\n/));
}

function validateMetadata(filePath, lines, errors) {
  if (isArchive(filePath) || !isDocsFile(filePath)) {
    return;
  }
  const metadata = parseMetadata(filePath, lines, errors);
  if (!metadata) {
    addError(errors, filePath, 1, 'active Markdown requires metadata with title, type, status and updated');
    return;
  }

  for (const key of ['title', 'type', 'status', 'updated']) {
    if (!metadata.get(key)) {
      addError(errors, filePath, 1, `document metadata is missing ${key}`);
    }
  }
  const type = metadata.get('type');
  if (type && !allowedTypes.has(normalizeType(type))) {
    addError(errors, filePath, 1, `document metadata type is not allowed: ${type}`);
  }
  const status = metadata.get('status');
  if (status && !allowedStatuses.has(normalizeStatus(status))) {
    addError(errors, filePath, 1, `document metadata status is not allowed: ${status}`);
  }
  const relativePath = displayPath(filePath);
  if (relativePath.startsWith('docs/proposals/') && !relativePath.endsWith('/README.md')) {
    if (type && normalizeType(type) !== 'proposal') {
      addError(errors, filePath, 1, 'proposal documents must use type: proposal');
    }
    if (status && normalizeStatus(status) !== 'proposed') {
      addError(errors, filePath, 1, 'proposal documents must use status: proposed');
    }
  }
  const updated = metadata.get('updated');
  if (updated && !/^\d{4}-\d{2}-\d{2}$/.test(updated)) {
    addError(errors, filePath, 1, 'document metadata updated must use YYYY-MM-DD');
  }
}

function validateHeadings(filePath, lines, errors) {
  for (const heading of parseHeadings(lines)) {
    if (!heading.slug) {
      addError(errors, filePath, heading.lineNumber, 'heading produces an empty anchor');
    }
  }
  for (const duplicate of findDuplicateHeadingAnchors(lines)) {
    addError(errors, filePath, duplicate.lineNumber, `duplicate heading anchor #${duplicate.slug} (first used at line ${duplicate.firstLine})`);
  }
}

function validateLinks(filePath, lines, errors, projectRoot = rootDir) {
  for (const link of parseLinks(lines)) {
    const resolved = resolveLinkTarget(link.target, filePath, projectRoot);
    if (resolved.kind === 'external') {
      continue;
    }
    if (resolved.kind === 'error') {
      addError(errors, filePath, link.lineNumber, `${resolved.message}: ${link.target}`);
      continue;
    }

    let destination = resolved.realPath ?? resolved.path;
    const stats = safeStat(destination, filePath, link.lineNumber, link.target, errors);
    if (!stats) {
      continue;
    }
    const realDestination = safeRealpath(destination, filePath, link.lineNumber, link.target, projectRoot, errors);
    if (realDestination) {
      destination = realDestination;
    }
    if (stats.isDirectory()) {
      const readme = join(destination, 'README.md');
      const index = join(destination, 'index.md');
      const readmeStats = statIfPresent(readme);
      let child = readme;
      if (!readmeStats || !readmeStats.isFile()) {
        const indexStats = statIfPresent(index);
        child = index;
        if (!indexStats || !indexStats.isFile()) {
          addError(errors, filePath, link.lineNumber, `directory link must contain README.md or index.md: ${link.target}`);
          continue;
        }
      }
      const realChild = safeRealpath(child, filePath, link.lineNumber, link.target, projectRoot, errors);
      if (!realChild) {
        continue;
      }
      destination = realChild;
    } else if (!stats.isFile()) {
      addError(errors, filePath, link.lineNumber, `local link target is not a regular file or directory: ${link.target}`);
      continue;
    }

    if (resolved.fragment) {
      const headings = readHeadings(destination, filePath, link.lineNumber, link.target, errors);
      if (!headings) {
        continue;
      }
      const slug = headingSlug(resolved.fragment);
      if (!headings.some((heading) => heading.slug === slug)) {
        addError(errors, filePath, link.lineNumber, `local link fragment does not exist: ${link.target}`);
      }
    }
  }
}

function validateLegacyTerminology(filePath, lines, errors) {
  if (isArchive(filePath)) {
    return;
  }
  forEachMarkdownLine(lines, (line, index) => {
    if (line.includes('/bridges') && !legacyMarker.test(line)) {
      addError(errors, filePath, index + 1, 'the legacy /bridges term must be explicitly marked as historical or compatibility-only');
    }
  });
}

function runChecks() {
  const errors = [];
  const rootMarkdownFiles = ['README.md', 'CONTRIBUTING.md', 'AGENTS.md', 'agent.md', 'SECURITY.md']
    .map((fileName) => join(rootDir, fileName))
    .filter((filePath) => {
      try {
        return statSync(filePath).isFile();
      } catch {
        return false;
      }
    });
  const markdownFiles = [...rootMarkdownFiles, ...collectMarkdown(join(rootDir, 'docs'), errors)].sort();
  for (const filePath of markdownFiles) {
    let content;
    try {
      content = readFileSync(filePath, 'utf8');
    } catch (error) {
      const code = error?.code ?? error?.message ?? String(error);
      addError(errors, filePath, 1, `unable to read Markdown file (${code})`);
      continue;
    }
    const lines = content.split(/\r?\n/);
    validateMetadata(filePath, lines, errors);
    validateHeadings(filePath, lines, errors);
    validateLinks(filePath, lines, errors);
    validateLegacyTerminology(filePath, lines, errors);
  }
  return { markdownFiles, errors };
}

function main() {
  const result = runChecks();
  if (result.errors.length > 0) {
    console.error(`Documentation checks failed with ${result.errors.length} error(s):`);
    for (const error of result.errors) {
      console.error(`- ${error}`);
    }
    process.exitCode = 1;
  } else {
    console.log(`Documentation checks passed (${result.markdownFiles.length} Markdown files).`);
  }
}

export {
  headingSlug,
  findDuplicateHeadingAnchors,
  parseHeadings,
  parseLinks,
  resolveLinkTarget,
  runChecks,
};

const currentModule = resolve(fileURLToPath(import.meta.url));
if (process.argv[1] && resolve(process.argv[1]) === currentModule) {
  main();
}
