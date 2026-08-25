import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';

import {
  findDuplicateHeadingAnchors,
  headingSlug,
  parseHeadings,
  parseLinks,
  resolveLinkTarget,
} from './check-docs.mjs';

test('Markdown link extraction ignores inline code, fenced code, and indented code', () => {
  const lines = [
    'Inline 文本 `[fake](fake.md)` and [real](nested/file(v2).md).',
    '```md',
    '[fenced](fenced.md)',
    '````',
    '[outside](outside.md)',
    '~~~js',
    '[tilde](tilde.md)',
    '~~~~',
    '    [indented](indented.md)',
  ];

  assert.deepEqual(
    parseLinks(lines).map(({ target }) => target),
    ['nested/file(v2).md', 'outside.md'],
  );
});

test('Markdown link extraction supports balanced destinations and angle destinations', () => {
  const links = parseLinks([
    '[nested](guides/file(v2)/(draft).md#Mixed%20Case)',
    '[spaced](<guides/file with spaces.md> "title")',
    '[reference]: guides/reference(file).md "title"',
  ]);

  assert.deepEqual(links.map(({ target }) => target), [
    'guides/file(v2)/(draft).md#Mixed%20Case',
    'guides/file with spaces.md',
    'guides/reference(file).md',
  ]);
});

test('an unmatched inline-code marker does not hide following Markdown links', () => {
  assert.deepEqual(parseLinks(['Unmatched ` marker and [real](outside.md).']).map(({ target }) => target), [
    'outside.md',
  ]);
});

test('heading anchors preserve underscores and Unicode while normalizing case and punctuation', () => {
  assert.equal(headingSlug('Hello_World'), 'hello_world');
  assert.equal(headingSlug('Café 标题'), 'café-标题');
  assert.equal(headingSlug('A heading: with punctuation!'), 'a-heading-with-punctuation');
  assert.equal(headingSlug(decodeURIComponent('Caf%C3%A9_%E6%A0%87%E9%A2%98')), 'café_标题');

  const headings = parseHeadings([
    '# Hello_World',
    '## hello_world',
    '### Café 标题',
    '#### `not` [extra](ignored)',
  ]);
  assert.deepEqual(findDuplicateHeadingAnchors([
    '# Hello_World',
    '## hello_world',
    '### Café 标题',
    '#### fenced',
  ]), [{ slug: 'hello_world', firstLine: 1, lineNumber: 2 }]);
  assert.deepEqual(headings.map(({ slug }) => slug), [
    'hello_world',
    'hello_world',
    'café-标题',
    'not-extra',
  ]);
});

test('heading extraction respects fence marker character and length', () => {
  const headings = parseHeadings([
    '```js',
    '# hidden short fence',
    '````',
    '# visible heading',
    '~~~md',
    '# hidden tilde heading',
    '~~~~',
    '    # hidden indented heading',
  ]);

  assert.deepEqual(headings.map(({ text }) => text), ['visible heading']);
});

test('link resolution rejects absolute paths, invalid encoding, and lexical root escapes', () => {
  const projectRoot = mkdtempSync(join(tmpdir(), 'agenthub-docs-'));
  const sourceFile = join(projectRoot, 'docs', 'source.md');
  mkdirSync(join(projectRoot, 'docs'), { recursive: true });
  writeFileSync(sourceFile, '# source\n');

  assert.equal(resolveLinkTarget('/etc/passwd', sourceFile, projectRoot).code, 'absolute-path');
  assert.equal(resolveLinkTarget('C:\\secret.md', sourceFile, projectRoot).code, 'absolute-path');
  assert.equal(resolveLinkTarget('file:///etc/passwd', sourceFile, projectRoot).code, 'absolute-path');
  assert.equal(resolveLinkTarget('../../secret.md', sourceFile, projectRoot).code, 'outside-root');
  assert.equal(resolveLinkTarget('../%2e%2e/secret.md', sourceFile, projectRoot).code, 'outside-root');
  assert.equal(resolveLinkTarget('bad%2', sourceFile, projectRoot).code, 'invalid-encoding');
  assert.equal(resolveLinkTarget('#Mixed%20Case', sourceFile, projectRoot).path, sourceFile);
  assert.equal(resolveLinkTarget('#Mixed%20Case', sourceFile, projectRoot).fragment, 'Mixed Case');
});

test('link resolution rejects a symlink whose realpath escapes the repository', (t) => {
  const projectRoot = mkdtempSync(join(tmpdir(), 'agenthub-docs-'));
  const outsideRoot = mkdtempSync(join(tmpdir(), 'agenthub-docs-outside-'));
  const docsRoot = join(projectRoot, 'docs');
  const sourceFile = join(docsRoot, 'source.md');
  const outsideFile = join(outsideRoot, 'secret.md');
  const linkFile = join(docsRoot, 'linked.md');
  mkdirSync(docsRoot, { recursive: true });
  writeFileSync(sourceFile, '# source\n');
  writeFileSync(outsideFile, '# secret\n');

  try {
    symlinkSync(outsideFile, linkFile, 'file');
  } catch (error) {
    t.skip(`symlink creation unavailable: ${error?.code ?? error?.message ?? String(error)}`);
    return;
  }

  assert.equal(resolveLinkTarget('linked.md', sourceFile, projectRoot).code, 'realpath-outside-root');
});

test('path fixture stays inside the temporary project root', () => {
  const projectRoot = resolve(mkdtempSync(join(tmpdir(), 'agenthub-docs-')));
  const sourceFile = join(projectRoot, 'docs', 'source.md');
  assert.equal(relative(projectRoot, sourceFile).replaceAll('\\', '/'), 'docs/source.md');
});
