/**
 * Centralized query-key factory for all React Query cache entries.
 * Every query key uses a hierarchical ['domain', 'sub', ...params] scheme
 * so that invalidation can target a specific entry or an entire domain.
 */
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
    list: (params?: { status?: string; page?: number; q?: string; type?: string }) =>
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
    sessions: (id: number) => ['scout', 'sessions', id] as const,
    research: () => ['scout', 'research'] as const,
    researchItems: (id: number) => ['scout', 'research', id, 'items'] as const,
  },

  // ── Sessions ──
  sessions: {
    all: ['sessions'] as const,
    list: (page: number, category?: string, status?: string) =>
      ['sessions', 'list', page, category ?? 'all', status ?? 'all'] as const,
    transcript: (sessionId: string) => ['sessions', 'transcript', sessionId] as const,
  },

  // ── Terminals ──
  terminals: {
    all: ['terminals'] as const,
    list: () => ['terminals', 'list'] as const,
  },

  // ── Workbenches ──
  workbenches: {
    all: ['workbenches'] as const,
    list: () => ['workbenches', 'list'] as const,
  },

  // ── Stats ──
  stats: {
    all: ['stats'] as const,
    activity: () => ['stats', 'activity'] as const,
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
} as const;
