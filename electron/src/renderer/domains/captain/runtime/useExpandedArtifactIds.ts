import { useCallback, useMemo } from 'react';
import type { FeedItem, TaskArtifact } from '#renderer/global/types';

// Evidence media classification — kept in sync with captain's typed evidence
// gates (review_phase_artifacts.rs). `gif` counts as a recording (animated
// screencast), even though the renderer uses an <img> tag for it.
const SCREENSHOT_EXTS = ['png', 'jpg', 'jpeg', 'webp'];
const RECORDING_EXTS = ['gif', 'mp4', 'mov', 'webm'];

/**
 * Decide which feed artifacts should expand by default.
 *
 * Non-evidence artifact types (work_summary, ...) follow the original rule:
 * the latest of each type expands.
 *
 * Evidence artifacts get type-aware expansion: the latest artifact containing
 * a screenshot AND the latest artifact containing a recording both expand,
 * so a UI task always shows both kinds proactively even when the worker
 * uploaded them as separate artifacts. If both kinds live in a single bundled
 * artifact, that one block expands and renders both. Older iteration
 * screenshots collapse on their own. Text-only evidence (terminal output)
 * also expands its latest, covering non-UI tasks.
 */
export function useExpandedArtifactIds(feedItems: FeedItem[]): (id: number) => boolean {
  const expandedIds = useMemo(() => {
    const ids = new Set<number>();
    const latestPerNonEvidenceType = new Map<string, number>();
    let latestScreenshotEvidence: number | null = null;
    let latestRecordingEvidence: number | null = null;
    let latestOtherEvidence: number | null = null;
    for (const fi of feedItems) {
      if (fi.type !== 'artifact') continue;
      const a = fi.data as TaskArtifact;
      if (a.artifact_type !== 'evidence') {
        latestPerNonEvidenceType.set(a.artifact_type, a.id);
        continue;
      }
      // Lowercase to match review_phase_artifacts.rs: `ext` is stored verbatim
      // from Path::extension(), so an uppercase extension (e.g. "MP4") would
      // otherwise miss the kind match and leave the artifact collapsed.
      const exts = (a.media ?? []).map((m) => m.ext.toLowerCase());
      const hasScreenshot = exts.some((e) => SCREENSHOT_EXTS.includes(e));
      const hasRecording = exts.some((e) => RECORDING_EXTS.includes(e));
      if (hasScreenshot) latestScreenshotEvidence = a.id;
      if (hasRecording) latestRecordingEvidence = a.id;
      if (!hasScreenshot && !hasRecording) latestOtherEvidence = a.id;
    }
    for (const id of latestPerNonEvidenceType.values()) ids.add(id);
    if (latestScreenshotEvidence !== null) ids.add(latestScreenshotEvidence);
    if (latestRecordingEvidence !== null) ids.add(latestRecordingEvidence);
    if (latestOtherEvidence !== null) ids.add(latestOtherEvidence);
    return ids;
  }, [feedItems]);
  return useCallback((id: number) => expandedIds.has(id), [expandedIds]);
}
