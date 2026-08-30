import type { FeedItem, ItemStatus, TaskArtifact, TimelineEvent } from '#renderer/global/types';

/** Renderer-only feed item that bundles consecutive evidence artifacts into
 *  one visual card. Computed at render time from the wire FeedItem stream;
 *  the daemon and SSE cache shape are unchanged. */
export type RenderableFeedItem =
  | FeedItem
  | { type: 'evidence-group'; timestamp: string; artifacts: TaskArtifact[] };

/** Merge the LAST TWO feed items into a single `evidence-group` iff they
 *  form a labeled before/after pair — both are evidence artifacts, one
 *  carries a `before_fix` media kind, the other carries `after_fix`. Any
 *  other shape of trailing items stays chronological so iterative
 *  re-shoots (task 103) and exploratory untyped uploads preceding a pair
 *  are not re-fused into the merged card. PR #1038's pairing case
 *  (two consecutive `mando todo evidence --kind` calls) still merges.
 *
 *  Suppressed timeline events (`human_ask`) are dropped up front:
 *  FeedBlocks renders them as null but they still occupy slots in
 *  `feedItems`, so a suppressed tail item would otherwise starve the
 *  last-pair check. */
export function groupEvidenceArtifacts(feedItems: FeedItem[]): RenderableFeedItem[] {
  const visible = feedItems.filter(
    (fi) => !(fi.type === 'timeline' && shouldSuppressTimelineEvent(fi.data.data.event_type)),
  );
  const n = visible.length;
  if (n < 2) return visible;

  const last = visible[n - 1];
  const prev = visible[n - 2];
  if (
    last.type !== 'artifact' ||
    last.data.artifact_type !== 'evidence' ||
    prev.type !== 'artifact' ||
    prev.data.artifact_type !== 'evidence'
  ) {
    return visible;
  }

  const lastKinds = new Set((last.data.media ?? []).map((m) => m.kind));
  const prevKinds = new Set((prev.data.media ?? []).map((m) => m.kind));
  const isPair =
    (lastKinds.has('before_fix') && prevKinds.has('after_fix')) ||
    (lastKinds.has('after_fix') && prevKinds.has('before_fix'));
  if (!isPair) return visible;

  const latest =
    last.data.created_at > prev.data.created_at ? last.data.created_at : prev.data.created_at;
  return [
    ...visible.slice(0, n - 2),
    { type: 'evidence-group', timestamp: latest, artifacts: [prev.data, last.data] },
  ];
}

export const EVENT_ICON_MAP: Record<string, ItemStatus> = {
  created: 'queued',
  worker_spawned: 'in-progress',
  // Plan mode was removed and nothing emits this any more, but timelines
  // written before the removal still carry `planning_spawned` rows and should
  // keep their icon. The map is string-keyed, so the retired wire variant
  // costs nothing here.
  planning_spawned: 'in-progress',
  worker_nudged: 'in-progress',
  worker_nudge_failed: 'errored',
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
 *  (auto_merge_triage carries no confidence on the current wire). */
export function confidenceIconOverride(event: TimelineEvent): ItemStatus | null {
  if (event.data.event_type !== 'awaiting_review') return null;
  const confidence = event.data.confidence.trim();
  if (confidence === 'high') return 'merged';
  if (confidence === 'low') return 'errored';
  return null;
}

/** Inline preview parts for verdict events -- the confidence grade and
 *  optional LLM-authored reason. Caller composes the rendered line so it
 *  can render the LLM `reason` through markdown without letting the
 *  fixed `Confidence:` prefix or the grade get parsed as syntax. */
export interface ConfidencePreview {
  confidence: string;
  reason: string;
}
export function confidencePreview(event: TimelineEvent): ConfidencePreview | null {
  const payload = event.data;
  if (payload.event_type !== 'awaiting_review') return null;
  const confidence = payload.confidence.trim();
  if (!confidence) return null;
  return { confidence, reason: payload.confidence_reason.trim() };
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
  return eventType === 'human_ask';
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
