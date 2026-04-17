import type { TimelineEvent } from '#renderer/global/types';

export const EVENT_ICON_MAP: Record<string, string> = {
  created: 'queued',
  worker_spawned: 'in-progress',
  worker_completed: 'awaiting-review',
  captain_review_started: 'captain-reviewing',
  captain_review_verdict: 'captain-reviewing',
  captain_merge_started: 'captain-merging',
  awaiting_review: 'awaiting-review',
  auto_merge_triage: 'captain-reviewing',
  auto_merge_triage_failed: 'errored',
  auto_merge_triage_exhausted: 'escalated',
  merged: 'merged',
  escalated: 'escalated',
  errored: 'errored',
  canceled: 'canceled',
  human_reopen: 'queued',
  human_ask: 'awaiting-review',
  rework_requested: 'rework',
  evidence_updated: 'awaiting-review',
  work_summary_updated: 'awaiting-review',
  planning_round: 'in-progress',
  plan_completed: 'plan-ready',
  plan_ready: 'plan-ready',
};

/** Color-code any confidence-bearing event by its grade:
 *  high -> green check (merge-ready), low -> red x (forced ship), mid /
 *  absent -> default icon. Works for both the new captain verdict on
 *  awaiting_review and the historical auto_merge_triage event. */
export function confidenceIconOverride(event: TimelineEvent): string | null {
  const eligible =
    event.event_type === 'awaiting_review' || event.event_type === 'auto_merge_triage';
  if (!eligible) return null;
  const confidence = event.data?.confidence as string | undefined;
  if (confidence === 'high') return 'merged';
  if (confidence === 'low') return 'errored';
  return null;
}

/** Inline preview line for verdict events -- shows the confidence grade +
 *  reason directly under the summary so the human doesn't have to click
 *  through. Handles the new captain verdict on `awaiting_review`, plus
 *  historical auto_merge_triage rows. Returns null otherwise. */
export function confidencePreview(event: TimelineEvent): string | null {
  const data = event.data as Record<string, unknown> | null | undefined;
  if (!data) return null;
  switch (event.event_type) {
    case 'awaiting_review': {
      const confidence = data.confidence;
      const reason = data.confidence_reason;
      if (typeof confidence !== 'string' || !confidence.trim()) return null;
      const reasonText = typeof reason === 'string' && reason.trim() ? reason : '';
      return reasonText ? `Confidence: ${confidence} — ${reasonText}` : `Confidence: ${confidence}`;
    }
    case 'auto_merge_triage': {
      const confidence = data.confidence;
      const reason = data.reason;
      const prefix =
        typeof confidence === 'string' && confidence.trim() ? `Confidence: ${confidence} — ` : '';
      return typeof reason === 'string' && reason.trim() ? `${prefix}${reason}` : null;
    }
    case 'auto_merge_triage_failed': {
      const error = data.error;
      return typeof error === 'string' && error.trim() ? error : null;
    }
    case 'auto_merge_triage_exhausted': {
      const lastError = data.last_error;
      if (typeof lastError === 'string' && lastError.trim()) {
        return `Last error: ${lastError}`;
      }
      return 'Human review needed';
    }
    default:
      return null;
  }
}

export function formatEventTime(timestamp: string): string {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function getNudgeReason(event: TimelineEvent): string | null {
  if (event.event_type !== 'worker_nudged') return null;
  return (
    (event.data?.reason as string | null | undefined) ??
    ((event.data?.content as string | null | undefined)
      ? firstLine(event.data.content as string, 140)
      : null)
  );
}

export function shouldSuppressTimelineEvent(eventType: string): boolean {
  return (
    eventType === 'work_summary_updated' ||
    eventType === 'evidence_updated' ||
    eventType === 'human_ask'
  );
}

/** Finds the timestamp of the latest clarify_question event in a feed. */
export function latestClarifyTimestamp(
  feedItems: { type: string; timestamp: string; data: unknown }[],
): string | null {
  for (let i = feedItems.length - 1; i >= 0; i--) {
    const fi = feedItems[i];
    if (fi.type === 'timeline' && (fi.data as TimelineEvent).event_type === 'clarify_question') {
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
