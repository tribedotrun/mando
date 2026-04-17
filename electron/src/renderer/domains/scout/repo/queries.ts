import { useQuery, keepPreviousData } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';
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

export type { ScoutQueryParams };

export function useScoutList(params?: ScoutQueryParams) {
  return useQuery<ScoutResponse>({
    queryKey: queryKeys.scout.list({
      status: params?.status,
      page: params?.page,
      q: params?.q,
      type: params?.type,
    }),
    queryFn: () => fetchScoutItems(params),
    placeholderData: keepPreviousData,
  });
}

export function useScoutItem(itemId: number, options?: { enabled?: boolean }) {
  return useQuery<ScoutItem>({
    queryKey: queryKeys.scout.item(itemId),
    queryFn: () => fetchScoutItem(itemId),
    enabled: options?.enabled,
  });
}

export function useScoutArticle(itemId: number) {
  return useQuery<ScoutArticleResponse>({
    queryKey: queryKeys.scout.article(itemId),
    queryFn: () => fetchScoutArticle(itemId),
  });
}

export function useResearchRuns() {
  return useQuery<ScoutResearchRun[]>({
    queryKey: queryKeys.scout.research(),
    queryFn: fetchResearchRuns,
  });
}

export function useResearchRunItems(runId: number) {
  return useQuery<ScoutItem[]>({
    queryKey: queryKeys.scout.researchItems(runId),
    queryFn: () => fetchResearchRunItems(runId),
    enabled: runId > 0,
  });
}
