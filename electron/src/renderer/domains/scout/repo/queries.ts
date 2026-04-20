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
    queryFn: () => toReactQuery(fetchScoutItems(params)),
    placeholderData: keepPreviousData,
  });
}

export function useScoutItem(itemId: number, options?: { enabled?: boolean }) {
  return useQuery<ScoutItem>({
    queryKey: queryKeys.scout.item(itemId),
    queryFn: () => toReactQuery(fetchScoutItem(itemId)),
    enabled: options?.enabled,
  });
}

export function useScoutArticle(itemId: number) {
  return useQuery<ScoutArticleResponse>({
    queryKey: queryKeys.scout.article(itemId),
    queryFn: () => toReactQuery(fetchScoutArticle(itemId)),
  });
}

export function useResearchRuns() {
  return useQuery<ScoutResearchRun[]>({
    queryKey: queryKeys.scout.research(),
    queryFn: () => toReactQuery(fetchResearchRuns()),
  });
}

export function useResearchRunItems(runId: number) {
  return useQuery<ScoutItem[]>({
    queryKey: queryKeys.scout.researchItems(runId),
    queryFn: () => toReactQuery(fetchResearchRunItems(runId)),
    enabled: runId > 0,
  });
}
