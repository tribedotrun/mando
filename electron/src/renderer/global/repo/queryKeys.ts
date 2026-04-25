/**
 * Centralized query-key factory for all React Query cache entries.
 * Every query key uses a hierarchical ['domain', 'sub', ...params] scheme
 * so that invalidation can target a specific entry or an entire domain.
 */
import type {
  ScoutItemStatusFilter,
  SessionCategory,
  SessionStatus,
  WorkbenchStatusFilter,
} from '#renderer/global/types';

type SessionStatusFilter = 'all' | SessionStatus;

export const queryKeys = {
  // ── Tasks ──
  tasks: {
    all: ['tasks'] as const,
    list: () => ['tasks', 'list'] as const,
    timeline: (id: number) => ['tasks', 'timeline', id] as const,
    pr: (id: number) => ['tasks', 'pr', id] as const,
    askHistory: (id: number) => ['tasks', 'ask-history', id] as const,
    feed: (id: number) => ['tasks', 'feed', id] as const,
    artifacts: (id: number) => ['tasks', 'artifacts', id] as const,
  },

  // ── Scout ──
  scout: {
    all: ['scout'] as const,
    list: (params?: { status?: ScoutItemStatusFilter; page?: number; q?: string; type?: string }) =>
      [
        'scout',
        'list',
        params?.status ?? 'all',
        params?.page ?? 0,
        params?.q ?? '',
        params?.type ?? '',
      ] as const,
    item: (id: number) => ['scout', 'item', id] as const,
    article: (id: number) => ['scout', 'article', id] as const,
    research: () => ['scout', 'research'] as const,
    researchItems: (id: number) => ['scout', 'research', id, 'items'] as const,
  },

  // ── Sessions ──
  sessions: {
    all: ['sessions'] as const,
    list: (page: number, category?: SessionCategory, status?: SessionStatusFilter) =>
      ['sessions', 'list', page, category ?? 'all', status ?? 'all'] as const,
    events: (sessionId: string) => ['sessions', 'events', sessionId] as const,
    jsonlPath: (sessionId: string) => ['sessions', 'jsonl-path', sessionId] as const,
  },

  // ── Terminals ──
  terminals: {
    all: ['terminals'] as const,
    list: () => ['terminals', 'list'] as const,
  },

  // ── Workbenches ──
  workbenches: {
    all: ['workbenches'] as const,
    list: (status?: WorkbenchStatusFilter) =>
      !status || status === 'active'
        ? (['workbenches', 'list'] as const)
        : (['workbenches', 'list', status] as const),
  },

  // ── Stats ──
  stats: {
    all: ['stats'] as const,
    activity: () => ['stats', 'activity'] as const,
  },

  health: {
    all: ['health'] as const,
    telegram: () => ['health', 'telegram'] as const,
  },

  // ── Workers (metrics) ──
  workers: {
    all: ['workers'] as const,
    list: () => ['workers', 'list'] as const,
  },

  // ── Config / Settings ──
  config: {
    all: ['config'] as const,
    current: () => ['config', 'current'] as const,
  },

  // ── Credentials ──
  credentials: {
    all: ['credentials'] as const,
    list: () => ['credentials', 'list'] as const,
  },

  // ── Highlighter ──
  highlighter: {
    all: ['shiki-highlight'] as const,
    code: (lang: string, code: string) => ['shiki-highlight', lang, code] as const,
  },

  // ── Settings (renderer-local read-through caches) ──
  settings: {
    all: ['settings'] as const,
    generalSystemInfo: () => ['settings', 'general', 'systemInfo'] as const,
    aboutAppVersion: () => ['settings', 'about', 'appVersion'] as const,
    telegramHealth: (apiToken: string) => ['settings', 'telegram', 'health', apiToken] as const,
  },

  // ── Onboarding (renderer-local read-through caches) ──
  onboarding: {
    all: ['onboarding'] as const,
    claudeCheck: () => ['onboarding', 'claude-check'] as const,
    appInfo: () => ['onboarding', 'app-info'] as const,
  },
} as const;
