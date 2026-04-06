/**
 * Desktop notification payload shape (matches Rust NotificationPayload).
 * Shared between main and renderer so the IPC contract has a single source.
 */

export type NotifyLevel = 'Low' | 'Normal' | 'High' | 'Critical';

export type NotificationKind =
  | { type: 'Escalated'; item_id: string; summary?: string }
  | { type: 'NeedsClarification'; item_id: string; questions?: string }
  | {
      type: 'RateLimited';
      status: string;
      utilization?: number;
      resets_at?: number;
      rate_limit_type?: string;
      overage_status?: string;
      overage_resets_at?: number;
      overage_disabled_reason?: string;
    }
  | {
      type: 'ScoutProcessed';
      scout_id: number;
      title: string;
      relevance: number;
      quality: number;
      source_name?: string;
      telegraph_url?: string;
    }
  | {
      type: 'ScoutProcessFailed';
      scout_id: number;
      url: string;
      error: string;
    }
  | { type: 'Generic' };

export interface NotificationPayload {
  message: string;
  level: NotifyLevel;
  kind: NotificationKind;
  task_key?: string;
  reply_markup?: unknown;
}
