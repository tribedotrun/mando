import type { FeedItem, TimelineEvent } from '#renderer/global/types';

export const EVENT_ICON_MAP: Record<string, string> = {
  created: 'queued',
  worker_spawned: 'in-progress',
  planning_spawned: 'in-progress',
  worker_nudged: 'in-progress',
  worker_nudge_failed: 'errored',
  worker_completed: 'awaiting-review',
  captain_review_started: 'captain-reviewing',
  captain_review_merge_fail: 'captain-reviewing',
  captain_review_clarifier_fail: 'captain-reviewing',
  captain_review_ci_failure: 'captain-reviewing',
  captain_review_rebase_exhausted: 'captain-reviewing',
  captain_review_verdict: 'captain-reviewing',
  captain_review_retry: 'captain-reviewing',
  captain_merge_started: 'captain-merging',
  captain_merge_queued: 'captain-merging',
  captain_merge_retry: 'captain-merging',
  awaiting_review: 'awaiting-review',
  auto_merge_triage: 'captain-reviewing',
  auto_merge_triage_failed: 'errored',
  auto_merge_triage_exhausted: 'escalated',
  merged: 'merged',
  accepted_no_pr: 'merged',
  escalated: 'escalated',
  review_errored: 'errored',
  clarifier_failed: 'errored',
  canceled: 'canceled',
  canceled_by_human: 'canceled',
  human_reopen: 'queued',
  human_ask: 'awaiting-review',
  rework_requested: 'rework',
  evidence_updated: 'awaiting-review',
  work_summary_updated: 'awaiting-review',
  planning_round: 'in-progress',
  plan_completed: 'plan-ready',
  plan_ready: 'plan-ready',
  clarify_timeout: 'captain-reviewing',
  clarifier_completed_no_pr: 'merged',
  status_changed_by_command: 'queued',
  status_changed_queued: 'queued',
  status_changed_retry_merge: 'captain-merging',
  status_changed_clarifier_fail: 'captain-reviewing',
  rate_limit_cleared: 'queued',
};

/** Color-code any confidence-bearing event by its grade:
 *  high -> green check (merge-ready), low -> red x (forced ship), mid /
 *  absent -> default icon. Works for the captain verdict on awaiting_review
 *  (auto_merge_triage* variants carry no confidence on the current wire). */
export function confidenceIconOverride(event: TimelineEvent): string | null {
  if (event.data.event_type !== 'awaiting_review') return null;
  const confidence = event.data.confidence.trim();
  if (confidence === 'high') return 'merged';
  if (confidence === 'low') return 'errored';
  return null;
}

/** Inline preview line for verdict events -- shows the confidence grade +
 *  reason directly under the summary so the human doesn't have to click
 *  through. Returns null when the event type has no preview text. */
export function confidencePreview(event: TimelineEvent): string | null {
  const payload = event.data;
  if (payload.event_type === 'awaiting_review') {
    const confidence = payload.confidence.trim();
    if (!confidence) return null;
    const reason = payload.confidence_reason.trim();
    return reason ? `Confidence: ${confidence} — ${reason}` : `Confidence: ${confidence}`;
  }
  return null;
}

export function formatEventTime(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function getNudgeReason(event: TimelineEvent): string | null {
  if (event.data.event_type !== 'worker_nudged') return null;
  const reason = event.data.reason.trim();
  if (reason) return reason;
  const content = event.data.content.trim();
  return content ? firstLine(content, 140) : null;
}

export function shouldSuppressTimelineEvent(eventType: string): boolean {
  return (
    eventType === 'work_summary_updated' ||
    eventType === 'evidence_updated' ||
    eventType === 'human_ask'
  );
}

/** Finds the timestamp of the latest clarify_question event in a feed. */
export function latestClarifyTimestamp(feedItems: FeedItem[]): string | null {
  for (let i = feedItems.length - 1; i >= 0; i--) {
    const fi = feedItems[i];
    if (fi.type === 'timeline' && fi.data.data.event_type === 'clarify_question') {
      return fi.timestamp;
    }
  }
  return null;
}

export function firstLine(s: string, max: number): string {
  const line = s.split('\n').find((l) => l.trim().length > 0) ?? s;
  const trimmed = line.trim();
  return trimmed.length > max ? `${trimmed.slice(0, max).trimEnd()}…` : trimmed;
}
