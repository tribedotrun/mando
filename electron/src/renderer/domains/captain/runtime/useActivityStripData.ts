import { useMemo } from 'react';
import { useActivityStats } from '#renderer/domains/captain/repo/queries';
import {
  ACTIVITY_STRIP_DAYS,
  buildCountMap,
  lastNDays,
  computeThresholds,
  buildGrid,
  type ActivityStripData,
} from '#renderer/domains/captain/service/activityStrip';

export type { ActivityStripData };

export function useActivityStripData(): ActivityStripData {
  const { data } = useActivityStats();
  return useMemo(() => {
    const merges = data?.daily_merges ?? [];
    const map = buildCountMap(merges);
    const days = lastNDays(ACTIVITY_STRIP_DAYS);
    const counts = days.map((d) => map.get(d) ?? 0);
    return {
      grid: buildGrid(days),
      countMap: map,
      thresholds: computeThresholds(counts),
      hasMerges: counts.some((c) => c > 0),
    };
  }, [data?.daily_merges]);
}
