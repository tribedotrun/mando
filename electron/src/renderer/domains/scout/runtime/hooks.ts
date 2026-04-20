import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '#renderer/global/repo/queryKeys';

export {
  useScoutList,
  useScoutItem,
  useScoutArticle,
  useResearchRuns,
  useResearchRunItems,
} from '#renderer/domains/scout/repo/queries';
export type { ScoutQueryParams } from '#renderer/domains/scout/repo/queries';
export {
  useScoutAdd,
  useScoutBulkUpdate,
  useScoutBulkDelete,
  useScoutStatusUpdate,
  useScoutAct,
  useScoutResearch,
  useScoutAsk,
  useScoutPublishTelegraph,
} from '#renderer/domains/scout/repo/mutations';
export { useScoutQASession } from '#renderer/domains/scout/runtime/useScoutQASession';

/** Invalidates all scout queries. Wraps queryKeys so UI never imports repo. */
export function useScoutRefresh() {
  const qc = useQueryClient();
  return useCallback(() => {
    void qc.invalidateQueries({ queryKey: queryKeys.scout.all });
  }, [qc]);
}
