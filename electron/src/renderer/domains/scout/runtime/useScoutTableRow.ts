import { useScoutItem } from '#renderer/domains/scout/runtime/hooks';
import type { ScoutItem } from '#renderer/global/types';
import {
  SCOUT_TYPE_BADGE,
  SCOUT_STATUS_VARIANT,
  scoutItemDomain,
} from '#renderer/domains/scout/service/researchHelpers';

interface UseScoutTableRowOptions {
  item: ScoutItem;
  isExpanded: boolean;
}

export interface ScoutTableRowData {
  summaryContent: string | null | undefined;
  summaryLoading: boolean;
  summaryError: string | undefined;
  badge: (typeof SCOUT_TYPE_BADGE)[keyof typeof SCOUT_TYPE_BADGE];
  domain: string | null;
  statusVariant: 'default' | 'secondary' | 'destructive' | 'outline';
}

export function useScoutTableRow({ item, isExpanded }: UseScoutTableRowOptions): ScoutTableRowData {
  const hasSummary = !!item.has_summary;
  const summaryQuery = useScoutItem(item.id, { enabled: isExpanded && hasSummary });
  const summaryContent = summaryQuery.data?.summary;
  const summaryLoading = summaryQuery.isLoading;
  const summaryError = summaryQuery.error ? String(summaryQuery.error) : undefined;
  const badge = SCOUT_TYPE_BADGE[item.item_type ?? 'other'] ?? SCOUT_TYPE_BADGE.other;
  const domain = scoutItemDomain(item);
  const statusVariant = SCOUT_STATUS_VARIANT[item.status] ?? 'outline';

  return { summaryContent, summaryLoading, summaryError, badge, domain, statusVariant };
}
