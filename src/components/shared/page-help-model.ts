import type { MessageKey } from '@/lib/i18n';
import {
  ROUTES_ACTIVITY_PATH,
  ROUTES_BOARD_PATH,
  ROUTES_PATH,
  ROUTES_POOL_PATH,
  ROUTES_TOKENS_PATH,
  SUB2API_PATH,
} from '@/lib/routes-path';

export const PAGE_HELP_IDS = [
  'dashboard',
  'chat',
  'agents',
  'skillsLibrary',
  'skillsProject',
  'skillsMarket',
  'mcp',
  'projects',
  'plugins',
  'connections',
  'sub2api',
  'routesBoard',
  'routesPool',
  'routesTokens',
  'routesActivity',
  'settingsPreferences',
  'settingsLocal',
  'settingsBackups',
  'settingsAbout',
] as const;

export type PageHelpId = (typeof PAGE_HELP_IDS)[number];

export type PageHelpCopy = {
  title: MessageKey;
  intro: MessageKey;
  steps: readonly MessageKey[];
};

/** Step count per page; locale keys must match `step1`…`stepN`. */
export const PAGE_HELP_STEP_COUNT: Record<PageHelpId, 2 | 3 | 4 | 5> = {
  dashboard: 4,
  chat: 4,
  agents: 4,
  skillsLibrary: 4,
  skillsProject: 4,
  skillsMarket: 3,
  mcp: 4,
  projects: 4,
  plugins: 4,
  connections: 5,
  sub2api: 4,
  routesBoard: 3,
  routesPool: 4,
  routesTokens: 3,
  routesActivity: 3,
  settingsPreferences: 4,
  settingsLocal: 3,
  settingsBackups: 5,
  settingsAbout: 3,
};

function searchParams(search: string): URLSearchParams {
  return new URLSearchParams(search.startsWith('?') ? search.slice(1) : search);
}

function settingsHelpId(search: string): PageHelpId {
  const tab = searchParams(search).get('tab');
  if (tab === 'local' || tab === 'data') return 'settingsLocal';
  if (tab === 'backups') return 'settingsBackups';
  if (tab === 'about' || tab === 'security') return 'settingsAbout';
  return 'settingsPreferences';
}

function skillsHelpId(search: string): PageHelpId {
  const tab = searchParams(search).get('tab');
  if (tab === 'project') return 'skillsProject';
  if (tab === 'market') return 'skillsMarket';
  return 'skillsLibrary';
}

/** Map the current path (and query) to a page tutorial. Unknown paths use Dashboard. */
export function pageHelpIdFromPath(pathname: string, search = ''): PageHelpId {
  if (pathname === '/chat') return 'chat';
  if (pathname === '/agents') return 'agents';
  if (pathname === '/skills') return skillsHelpId(search);
  if (pathname === '/mcp') return 'mcp';
  if (pathname === '/projects') return 'projects';
  if (pathname === '/plugins') return 'plugins';
  if (pathname === '/connections') return 'connections';
  if (pathname === SUB2API_PATH) return 'sub2api';
  if (pathname === ROUTES_POOL_PATH || pathname.startsWith(`${ROUTES_POOL_PATH}/`)) {
    return 'routesPool';
  }
  if (pathname === ROUTES_TOKENS_PATH || pathname.startsWith(`${ROUTES_TOKENS_PATH}/`)) {
    return 'routesTokens';
  }
  if (pathname === ROUTES_ACTIVITY_PATH || pathname.startsWith(`${ROUTES_ACTIVITY_PATH}/`)) {
    return 'routesActivity';
  }
  if (
    pathname === ROUTES_PATH ||
    pathname === ROUTES_BOARD_PATH ||
    pathname.startsWith(`${ROUTES_BOARD_PATH}/`)
  ) {
    return 'routesBoard';
  }
  if (pathname === '/settings') return settingsHelpId(search);
  return 'dashboard';
}

export function pageHelpCopy(id: PageHelpId): PageHelpCopy {
  const n = PAGE_HELP_STEP_COUNT[id];
  const prefix = `chrome.pageHelp.pages.${id}`;
  return {
    title: `${prefix}.title` as MessageKey,
    intro: `${prefix}.intro` as MessageKey,
    steps: Array.from(
      { length: n },
      (_, i) => `${prefix}.step${i + 1}` as MessageKey,
    ),
  };
}

/** `data-help` names for each step. Missing controls are skipped. */
export const PAGE_HELP_TARGETS: Record<PageHelpId, readonly string[]> = {
  dashboard: ['dashboard-overview', 'dashboard-collect', 'dashboard-filters', 'dashboard-usage'],
  chat: ['chat-new', 'chat-cwd', 'chat-send', 'chat-settings'],
  agents: ['agents-env', 'list-row', 'inspect-panel', 'agents-hide'],
  skillsLibrary: ['skills-tabs', 'skills-matrix', 'list-row', 'inspect-panel'],
  skillsProject: ['skills-tabs', 'skills-workspace', 'list-row', 'inspect-panel'],
  skillsMarket: ['skills-tabs', 'skills-market', 'inspect-panel'],
  mcp: ['agent-tabs', 'page-refresh', 'list-row', 'mcp-list'],
  projects: ['agent-tabs', 'projects-search', 'list-row', 'inspect-panel'],
  plugins: ['agent-tabs', 'list-row', 'inspect-panel', 'page-refresh'],
  connections: ['agent-tabs', 'connections-add', 'list-row', 'inspect-panel', 'connections-trash'],
  sub2api: ['sub2api-login', 'list-row', 'inspect-panel', 'sub2api-import'],
  routesBoard: ['routes-board-switch', 'routes-board-endpoints', 'routes-board-usage'],
  routesPool: ['pool-add', 'list-row', 'inspect-panel', 'connections-trash'],
  routesTokens: ['list-row', 'tokens-copy', 'inspect-panel'],
  routesActivity: ['page-chrome', 'list-row', 'inspect-panel'],
  settingsPreferences: ['settings-tabs', 'settings-appearance', 'settings-sidebar', 'settings-usage'],
  settingsLocal: ['settings-tabs', 'settings-datadir', 'settings-logs'],
  settingsBackups: ['settings-tabs', 'backups-keep', 'backups-now', 'list-row', 'inspect-panel'],
  settingsAbout: ['settings-tabs', 'settings-version', 'settings-feedback'],
};

export function pageHelpStepSelector(id: PageHelpId, index: number): string {
  const name = PAGE_HELP_TARGETS[id][index];
  return name ? `[data-help="${name}"]` : '[data-page-help]';
}
