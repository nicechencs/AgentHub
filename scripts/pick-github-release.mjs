#!/usr/bin/env node
/**
 * Resolve a GitHub Release that may still be an untagged draft.
 *
 * `GET /releases/tags/:tag` and GraphQL `release(tagName:)` 404/miss drafts
 * whose html_url is `/releases/tag/untagged-*` even when the intended tag
 * already exists on origin. Match create output (id / html_url) or a listed
 * release by tag_name, title `AgentHub vX.Y.Z`, or that untagged html_url.
 */
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

function findHtmlReleaseUrls(text) {
  return String(text).match(/https:\/\/github\.com\/[^\s]+\/releases\/(?:tag\/[^\s/#]+)/gi);
}

function releaseTitle(tag) {
  return `AgentHub ${tag}`;
}

function isHtmlReleaseUrl(value) {
  return typeof value === 'string' && /https:\/\/github\.com\/[^/]+\/[^/]+\/releases\//i.test(value);
}

function normalizeUrl(value) {
  return String(value || '')
    .trim()
    .replace(/\/+$/, '');
}

function urlsEqual(left, right) {
  return normalizeUrl(left).toLowerCase() === normalizeUrl(right).toLowerCase();
}

function numericId(raw) {
  const candidates = [raw?.id, raw?.databaseId, raw?.database_id];
  for (const candidate of candidates) {
    if (typeof candidate === 'number' && Number.isInteger(candidate)) return candidate;
    if (typeof candidate === 'string' && /^\d+$/.test(candidate)) return Number(candidate);
  }
  return null;
}

function normalizeRelease(raw = {}) {
  const htmlUrl = raw.html_url || raw.htmlUrl || (isHtmlReleaseUrl(raw.url) ? raw.url : '');
  const tagName = raw.tag_name ?? raw.tagName ?? '';
  const publishedAt = raw.published_at ?? raw.publishedAt ?? '';
  return {
    id: numericId(raw),
    tag_name: typeof tagName === 'string' ? tagName : '',
    name: typeof raw.name === 'string' ? raw.name : typeof raw.title === 'string' ? raw.title : '',
    html_url: typeof htmlUrl === 'string' ? htmlUrl : '',
    draft: raw.draft === true || raw.isDraft === true,
    prerelease: raw.prerelease === true || raw.isPrerelease === true,
    published_at: publishedAt == null ? '' : String(publishedAt),
    assets: Array.isArray(raw.assets) ? raw.assets : [],
  };
}

function parseCreateOutput(text) {
  const trimmed = String(text ?? '').trim();
  if (!trimmed) return null;

  const jsonStart = trimmed.indexOf('{');
  if (jsonStart !== -1 && (trimmed.endsWith('}') || trimmed.includes('}'))) {
    const candidate = trimmed.slice(jsonStart);
    try {
      const parsed = JSON.parse(candidate);
      const object = Array.isArray(parsed) ? parsed[0] : parsed;
      if (object && typeof object === 'object') return normalizeRelease(object);
    } catch {
      // Fall through to URL matching when create stdout is not JSON.
    }
  }

  const urls = findHtmlReleaseUrls(trimmed);
  if (urls && urls.length > 0) {
    return normalizeRelease({ html_url: urls[urls.length - 1], draft: true });
  }
  return null;
}

function isUntaggedHtmlUrl(htmlUrl) {
  return /\/releases\/tag\/untagged-[0-9a-f]+/i.test(String(htmlUrl || ''));
}

function releaseMatches(release, { tag, title }) {
  if (tag && release.tag_name === tag) return true;
  if (title && release.name === title) return true;
  if (isUntaggedHtmlUrl(release.html_url) && title && release.name === title) return true;
  if (isUntaggedHtmlUrl(release.html_url) && tag && release.tag_name === tag) return true;
  return false;
}

function finalize(release, tag) {
  const htmlUrl = release.html_url || '';
  const ghTarget = htmlUrl || (release.id != null ? String(release.id) : release.tag_name || tag || '');
  return {
    id: release.id,
    tag_name: release.tag_name || '',
    name: release.name || '',
    html_url: htmlUrl,
    draft: release.draft === true,
    prerelease: release.prerelease === true,
    published_at: release.published_at || '',
    gh_target: ghTarget,
    assets: release.assets || [],
  };
}

function pickGitHubRelease({ tag, title, createOutput, releases = [] } = {}) {
  if (!tag || typeof tag !== 'string') {
    throw new Error('tag is required');
  }
  const expectedTitle = title || releaseTitle(tag);
  const normalized = (Array.isArray(releases) ? releases : []).map(normalizeRelease);
  const created = parseCreateOutput(createOutput);

  if (created) {
    if (created.id != null) {
      const byId = normalized.find((release) => release.id === created.id);
      return finalize(byId || created, tag);
    }
    if (created.html_url) {
      const byUrl = normalized.find((release) => urlsEqual(release.html_url, created.html_url));
      if (byUrl) return finalize(byUrl, tag);
      return finalize(
        {
          ...created,
          name: created.name || expectedTitle,
          tag_name: created.tag_name || '',
        },
        tag,
      );
    }
  }

  const matches = normalized.filter((release) => releaseMatches(release, { tag, title: expectedTitle }));
  if (matches.length === 0) return null;
  if (matches.length === 1) return finalize(matches[0], tag);

  const exact = matches.filter((release) => release.tag_name === tag && release.name === expectedTitle);
  if (exact.length === 1) return finalize(exact[0], tag);
  const untagged = matches.filter((release) => isUntaggedHtmlUrl(release.html_url));
  if (untagged.length === 1) return finalize(untagged[0], tag);
  throw new Error(`Multiple GitHub Releases match ${expectedTitle} / ${tag}`);
}

function parseReleasesJson(text) {
  const trimmed = String(text ?? '').trim();
  if (!trimmed) return [];
  const parsed = JSON.parse(trimmed);
  if (Array.isArray(parsed)) return parsed;
  if (parsed && typeof parsed === 'object' && Array.isArray(parsed.releases)) return parsed.releases;
  if (parsed && typeof parsed === 'object') return [parsed];
  throw new Error('releases JSON must be an array or a release object');
}

function parseCliArgs(argv) {
  const options = {
    tag: null,
    title: null,
    createOutput: null,
    createOutputText: null,
    releases: null,
    githubOutput: null,
    require: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const next = () => {
      const value = argv[index + 1];
      if (!value || value.startsWith('--')) throw new Error(`${argument} requires a value`);
      index += 1;
      return value;
    };
    if (argument === '--tag') options.tag = next();
    else if (argument === '--title') options.title = next();
    else if (argument === '--create-output') options.createOutput = next();
    else if (argument === '--create-output-text') options.createOutputText = next();
    else if (argument === '--releases') options.releases = next();
    else if (argument === '--github-output') options.githubOutput = next();
    else if (argument === '--require') options.require = true;
    else if (argument === '--help' || argument === '-h') {
      console.log(
        'Usage: node scripts/pick-github-release.mjs --tag TAG [--title TITLE] [--create-output FILE] [--releases FILE] [--github-output FILE] [--require]',
      );
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${argument}`);
    }
  }
  return options;
}

function formatGithubOutput(picked) {
  if (!picked) {
    return [
      'RELEASE_STATE=none',
      'RELEASE_ID=',
      'RELEASE_HTML_URL=',
      'RELEASE_GH_TARGET=',
      'RELEASE_DRAFT=',
      'RELEASE_PRERELEASE=',
      'RELEASE_PUBLISHED_AT=',
      'RELEASE_TAG_NAME=',
      'RELEASE_NAME=',
      '',
    ].join('\n');
  }
  const published = picked.draft !== true || picked.published_at;
  return [
    `RELEASE_STATE=${published ? 'published' : 'draft'}`,
    `RELEASE_ID=${picked.id ?? ''}`,
    `RELEASE_HTML_URL=${picked.html_url || ''}`,
    `RELEASE_GH_TARGET=${picked.gh_target || ''}`,
    `RELEASE_DRAFT=${picked.draft === true}`,
    `RELEASE_PRERELEASE=${picked.prerelease === true}`,
    `RELEASE_PUBLISHED_AT=${picked.published_at || ''}`,
    `RELEASE_TAG_NAME=${picked.tag_name || ''}`,
    `RELEASE_NAME=${picked.name || ''}`,
    '',
  ].join('\n');
}

function main() {
  const options = parseCliArgs(process.argv.slice(2));
  if (!options.tag) throw new Error('--tag is required');

  const createOutput = options.createOutputText
    ?? (options.createOutput ? fs.readFileSync(path.resolve(options.createOutput), 'utf8') : '');
  const releasesText = options.releases
    ? fs.readFileSync(path.resolve(options.releases), 'utf8')
    : fs.readFileSync(0, 'utf8');
  const picked = pickGitHubRelease({
    tag: options.tag,
    title: options.title || releaseTitle(options.tag),
    createOutput,
    releases: parseReleasesJson(releasesText),
  });

  if (options.require && !picked) {
    throw new Error(`No GitHub Release matched ${options.title || releaseTitle(options.tag)} / ${options.tag}`);
  }
  if (options.githubOutput) {
    fs.appendFileSync(path.resolve(options.githubOutput), formatGithubOutput(picked), 'utf8');
  }
  if (picked) process.stdout.write(`${JSON.stringify(picked)}\n`);
}

export {
  finalize,
  formatGithubOutput,
  isUntaggedHtmlUrl,
  normalizeRelease,
  parseCreateOutput,
  parseCliArgs,
  parseReleasesJson,
  pickGitHubRelease,
  releaseMatches,
  releaseTitle,
};

const invokedPath = process.argv[1] ? pathToFileURL(path.resolve(process.argv[1])).href : null;
if (invokedPath === import.meta.url) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
