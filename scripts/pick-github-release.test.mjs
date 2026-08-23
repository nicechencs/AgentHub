import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import {
  parseCreateOutput,
  pickGitHubRelease,
  releaseTitle,
} from './pick-github-release.mjs';

const helperPath = fileURLToPath(new URL('./pick-github-release.mjs', import.meta.url));

const TAG = 'v0.3.0';
const TITLE = 'AgentHub v0.3.0';
const UNTAGGED_URL = 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-e6b231ed2c771735f49b';

function untaggedDraft(overrides = {}) {
  return {
    id: 258901111,
    tag_name: '',
    name: TITLE,
    html_url: UNTAGGED_URL,
    draft: true,
    prerelease: false,
    published_at: null,
    assets: [{ name: 'AgentHub_0.3.0_x64-setup.exe' }],
    ...overrides,
  };
}

test('release title follows AgentHub vX.Y.Z', () => {
  assert.equal(releaseTitle('v0.3.0'), 'AgentHub v0.3.0');
  assert.equal(releaseTitle('v0.3.1'), 'AgentHub v0.3.1');
});

test('parses gh release create stdout URL for an untagged draft', () => {
  const parsed = parseCreateOutput(
    `https://github.com/nicechencs/AgentHub/releases/tag/untagged-e6b231ed2c771735f49b\n`,
  );
  assert.equal(parsed.html_url, UNTAGGED_URL);
  assert.equal(parsed.id, null);
});

test('parses gh --json create output id and html url', () => {
  const parsed = parseCreateOutput(
    JSON.stringify({
      databaseId: 258901111,
      url: UNTAGGED_URL,
      tagName: 'v0.3.0',
      name: TITLE,
      isDraft: true,
      isPrerelease: false,
    }),
  );
  assert.equal(parsed.id, 258901111);
  assert.equal(parsed.html_url, UNTAGGED_URL);
  assert.equal(parsed.tag_name, 'v0.3.0');
  assert.equal(parsed.draft, true);
});

test('picks an untagged draft by title AgentHub v0.3.0 when tag_name is empty', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    releases: [
      untaggedDraft(),
      {
        id: 1,
        tag_name: 'v0.2.9',
        name: 'AgentHub v0.2.9',
        html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/v0.2.9',
        draft: false,
      },
    ],
  });
  assert.equal(picked.id, 258901111);
  assert.equal(picked.name, TITLE);
  assert.equal(picked.draft, true);
  assert.equal(picked.gh_target, UNTAGGED_URL);
  assert.match(picked.html_url, /untagged-[0-9a-f]+/);
});

test('picks a draft by tag_name even when html_url is still untagged-*', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    releases: [
      untaggedDraft({
        tag_name: 'v0.3.0',
        name: '',
        html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-aaaaaaaaaaaaaaaaaaaa',
      }),
    ],
  });
  assert.equal(picked.tag_name, 'v0.3.0');
  assert.match(picked.html_url, /untagged-/);
});

test('picks from create output html_url containing untagged-*', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    createOutput: `https://github.com/nicechencs/AgentHub/releases/tag/untagged-e6b231ed2c771735f49b`,
    releases: [untaggedDraft({ name: '', tag_name: '' })],
  });
  assert.equal(picked.id, 258901111);
  assert.equal(picked.html_url, UNTAGGED_URL);
});

test('picks from create output JSON id over other listed drafts', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    createOutput: JSON.stringify({ databaseId: 42, url: UNTAGGED_URL, isDraft: true }),
    releases: [
      untaggedDraft({ id: 99, name: TITLE, html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-bbbbbbbbbbbbbbbbbbbb' }),
      untaggedDraft({ id: 42, name: TITLE }),
    ],
  });
  assert.equal(picked.id, 42);
});

test('create output URL is enough when the draft list has not been fetched yet', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    createOutput: UNTAGGED_URL,
    releases: [],
  });
  assert.equal(picked.id, null);
  assert.equal(picked.html_url, UNTAGGED_URL);
  assert.equal(picked.gh_target, UNTAGGED_URL);
  assert.equal(picked.name, TITLE);
});

test('does not treat another version untagged draft as this tag', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    releases: [
      untaggedDraft({
        id: 7,
        name: 'AgentHub v0.3.1',
        tag_name: 'v0.3.1',
        html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-cccccccccccccccccccc',
      }),
    ],
  });
  assert.equal(picked, null);
});

test('resumes an existing untagged draft instead of reporting missing', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    title: TITLE,
    releases: [untaggedDraft()],
  });
  assert.equal(picked.draft, true);
  assert.equal(picked.name, TITLE);
  assert.equal(picked.id, 258901111);
});

test('picks a published release by tag_name', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    releases: [
      {
        id: 10,
        tag_name: 'v0.3.0',
        name: TITLE,
        html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/v0.3.0',
        draft: false,
        prerelease: false,
        published_at: '2026-08-20T00:00:00Z',
      },
    ],
  });
  assert.equal(picked.draft, false);
  assert.equal(picked.tag_name, 'v0.3.0');
  assert.equal(picked.html_url, 'https://github.com/nicechencs/AgentHub/releases/tag/v0.3.0');
});

test('accepts gh release list camelCase fields', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    releases: [
      {
        databaseId: 258901111,
        tagName: '',
        name: TITLE,
        url: UNTAGGED_URL,
        isDraft: true,
        isPrerelease: false,
      },
    ],
  });
  assert.equal(picked.id, 258901111);
  assert.equal(picked.html_url, UNTAGGED_URL);
});

test('throws when two different drafts both match this version', () => {
  assert.throws(
    () =>
      pickGitHubRelease({
        tag: TAG,
        releases: [
          untaggedDraft({ id: 1, html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-aaaaaaaaaaaaaaaaaaaa' }),
          untaggedDraft({ id: 2, html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-bbbbbbbbbbbbbbbbbbbb' }),
        ],
      }),
    /Multiple GitHub Releases match AgentHub v0\.3\.0/,
  );
});

test('does not pick a random untagged draft without this version title or tag', () => {
  const picked = pickGitHubRelease({
    tag: TAG,
    releases: [
      untaggedDraft({
        id: 8,
        name: 'WIP',
        tag_name: '',
        html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-dddddddddddddddddddd',
      }),
    ],
  });
  assert.equal(picked, null);
});

test('CLI reads a REST draft list on stdin and prints the matching untagged release', () => {
  const result = spawnSync(process.execPath, [helperPath, '--tag', TAG], {
    encoding: 'utf8',
    input: JSON.stringify([untaggedDraft()]),
  });
  assert.equal(result.status, 0, result.stderr);
  const picked = JSON.parse(result.stdout);
  assert.equal(picked.id, 258901111);
  assert.equal(picked.name, TITLE);
  assert.match(picked.html_url, /untagged-e6b231ed2c771735f49b/);
});

test('CLI prefers create-output untagged html_url when listing drafts', () => {
  const result = spawnSync(
    process.execPath,
    [helperPath, '--tag', TAG, '--create-output-text', UNTAGGED_URL],
    {
      encoding: 'utf8',
      input: JSON.stringify([
        untaggedDraft({
          id: 99,
          name: TITLE,
          html_url: 'https://github.com/nicechencs/AgentHub/releases/tag/untagged-bbbbbbbbbbbbbbbbbbbb',
        }),
        untaggedDraft(),
      ]),
    },
  );
  assert.equal(result.status, 0, result.stderr);
  const picked = JSON.parse(result.stdout);
  assert.equal(picked.id, 258901111);
  assert.equal(picked.html_url, UNTAGGED_URL);
});
