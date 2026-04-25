import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { daemonSyncMeta } from '#renderer/global/repo/syncPolicy';
import {
  fetchScoutItems,
  fetchScoutItem,
  fetchScoutArticle,
  fetchResearchRuns,
  fetchResearchRunItems,
  type ScoutQueryParams,
} from '#renderer/domains/scout/repo/api';
import type {
  ScoutResponse,
  ScoutItem,
  ScoutArticleResponse,
  ScoutResearchRun,
} from '#renderer/global/types';
import { toReactQuery } from '#result';

export type { ScoutQueryParams };

export function useScoutList(params?: ScoutQueryParams) {
  return useQuery<ScoutResponse>({
    queryKey: queryKeys.scout.list({
      status: params?.status,
      page: params?.page,
      q: params?.q,
      type: params?.type,
    }),
    meta: daemonSyncMeta('sse-invalidated', 'scout entity events invalidate paginated lists'),
    queryFn: () => toReactQuery(fetchScoutItems(params)),
    placeholderData: keepPreviousData,
  });
}

export function useScoutItem(itemId: number, options?: { enabled?: boolean }) {
  return useQuery<ScoutItem>({
    queryKey: queryKeys.scout.item(itemId),
    meta: daemonSyncMeta('mutation-invalidated', 'scout item actions refresh detail on demand'),
    queryFn: () => toReactQuery(fetchScoutItem(itemId)),
    enabled: options?.enabled,
  });
}

export function useScoutArticle(itemId: number) {
  return useQuery<ScoutArticleResponse>({
    queryKey: queryKeys.scout.article(itemId),
    meta: daemonSyncMeta('manual', 'article body is fetched on open'),
    queryFn: () => toReactQuery(fetchScoutArticle(itemId)),
  });
}

export function useResearchRuns() {
  return useQuery<ScoutResearchRun[]>({
    queryKey: queryKeys.scout.research(),
    meta: daemonSyncMeta('sse-invalidated', 'research events invalidate run list'),
    queryFn: () => toReactQuery(fetchResearchRuns()),
  });
}

export function useResearchRunItems(runId: number) {
  return useQuery<ScoutItem[]>({
    queryKey: queryKeys.scout.researchItems(runId),
    meta: daemonSyncMeta('sse-invalidated', 'research events invalidate run items'),
    queryFn: () => toReactQuery(fetchResearchRunItems(runId)),
    enabled: runId > 0,
  });
}
