import type {
  ActResponse,
  ScoutItem,
  ScoutItemStatus,
  ScoutItemStatusFilter,
  ScoutResearchRun,
} from '#renderer/global/types';
import { getErrorMessage } from '#renderer/global/service/utils';

/** Formats the result of a scout "act" mutation for display. */
export function formatActResult(data: ActResponse | undefined, error: Error | null): string | null {
  if (data) return data.skipped ? `Skipped: ${data.reason}` : `Created task: ${data.title}`;
  if (error) return `Error: ${getErrorMessage(error, 'unknown')}`;
  return null;
}

/** Default number of scout items per page. */
export const SCOUT_DEFAULT_PER_PAGE = 25;

/** Statuses a user can manually set on scout items. */
export const USER_SETTABLE_STATUSES = [
  'pending',
  'processed',
  'saved',
  'archived',
] as const satisfies readonly ScoutItemStatus[];

export type ScoutUserSettableStatus = (typeof USER_SETTABLE_STATUSES)[number];

const USER_SETTABLE_STATUS_VALUES: readonly string[] = USER_SETTABLE_STATUSES;

export function isUserSettableScoutStatus(status: string): status is ScoutUserSettableStatus {
  return USER_SETTABLE_STATUS_VALUES.includes(status);
}

/** Badge variant for each scout item status. */
export const SCOUT_STATUS_VARIANT: Record<
  ScoutItemStatus,
  'default' | 'secondary' | 'destructive' | 'outline'
> = {
  pending: 'outline',
  fetched: 'secondary',
  processed: 'default',
  saved: 'secondary',
  archived: 'outline',
  error: 'destructive',
};

/** Type badge label for each scout item type. */
export const SCOUT_TYPE_BADGE: Record<string, { label: string; variant: 'outline' }> = {
  github: { label: 'GH', variant: 'outline' },
  youtube: { label: 'YT', variant: 'outline' },
  arxiv: { label: 'arXiv', variant: 'outline' },
  blog: { label: 'blog', variant: 'outline' },
  other: { label: '', variant: 'outline' },
};

/** Filter options for the scout item type dropdown. */
export const SCOUT_TYPE_OPTIONS = ['all', 'github', 'youtube', 'arxiv', 'other'] as const;

export type ScoutStatusFilter = ScoutItemStatusFilter;

/** Filter options for the scout item state dropdown. */
export const SCOUT_STATE_OPTIONS = [
  'all',
  'saved',
  'archived',
] as const satisfies readonly ScoutStatusFilter[];

/** Whether a scout item can have the "act" (create task) action performed. */
export function isScoutItemActionable(item: ScoutItem): boolean {
  return item.status === 'processed' || item.status === 'saved' || item.status === 'archived';
}

/** Display title for a scout item, with fallback for pending/untitled items. */
export function scoutItemTitle(item: Pick<ScoutItem, 'title' | 'status'>): string {
  return item.title || (item.status === 'pending' ? 'Pending processing\u2026' : 'Untitled');
}

export interface StatusBadgeConfig {
  variant: 'outline' | 'secondary' | 'destructive';
  label: string;
  spinning: boolean;
  showElapsed: boolean;
}

/** Extracts display domain from a scout item (source_name or URL hostname). */
export function scoutItemDomain(item: Pick<ScoutItem, 'source_name' | 'url'>): string {
  if (item.source_name) return item.source_name;
  if (!item.url) return '';
  try {
    return new URL(item.url).hostname.replace('www.', '');
  } catch {
    return '';
  }
}

export function statusBadgeConfig(status: ScoutResearchRun['status']): StatusBadgeConfig {
  switch (status) {
    case 'running':
      return { variant: 'outline', label: 'Running', spinning: true, showElapsed: true };
    case 'done':
      return { variant: 'secondary', label: 'Done', spinning: false, showElapsed: false };
    case 'failed':
      return { variant: 'destructive', label: 'Failed', spinning: false, showElapsed: false };
  }
  const exhaustive: never = status;
  return exhaustive;
}
