import { buildUrl } from '#renderer/global/runtime/useApi';
import { staticRoutePath } from '#renderer/global/providers/http';
import type { ArtifactMedia, TaskArtifact } from '#renderer/global/types';

export const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp'];
export const VIDEO_EXTS = ['mp4', 'mov', 'webm'];

/** A single media item paired with the id of the artifact it belongs to.
 *  Grouped evidence cards flatten media across multiple artifacts but the
 *  artifact-media static route still resolves per artifact, so each item
 *  must remember its origin id. */
interface MediaWithArtifactId {
  artifactId: number;
  media: ArtifactMedia;
}

interface ArtifactMediaDerived {
  hasVideo: boolean;
  imageUrls: string[];
  imageCaptions: string[];
  /** Maps `${artifactId}:${mediaIndex}` to position in `imageUrls`. The
   *  composite key is required because two grouped artifacts each carry
   *  their own 0,1,2... media indices. */
  lightboxKeyOf: Map<string, number>;
}

export const lightboxKey = (artifactId: number, mediaIndex: number): string =>
  `${artifactId}:${mediaIndex}`;

/** Header summary for an evidence card. Covers single-artifact and grouped
 *  cases identically; the grouped case sums counts and takes the latest
 *  timestamp / OR's `hasVideo` across the group. */
interface ArtifactGroupSummary {
  mediaCount: number;
  latestTimestamp: string;
  hasVideo: boolean;
}

export function summarizeArtifactGroup(artifacts: TaskArtifact[]): ArtifactGroupSummary {
  let mediaCount = 0;
  let latestTimestamp = '';
  let hasVideo = false;
  for (const a of artifacts) {
    const items = a.media ?? [];
    mediaCount += items.length;
    if (a.created_at > latestTimestamp) latestTimestamp = a.created_at;
    if (items.some((m) => VIDEO_EXTS.includes(m.ext))) hasVideo = true;
  }
  return { mediaCount, latestTimestamp, hasVideo };
}

/** Flatten media across one or more artifacts in artifact order. */
export function flattenArtifactMedia(artifacts: TaskArtifact[]): MediaWithArtifactId[] {
  const out: MediaWithArtifactId[] = [];
  for (const a of artifacts) {
    for (const m of a.media ?? []) {
      out.push({ artifactId: a.id, media: m });
    }
  }
  return out;
}

/** Lightbox-ready derivation. Walks one or more artifacts in order, building
 *  a flat image list keyed by `(artifactId, mediaIndex)` so a grouped card
 *  can navigate the lightbox across artifacts without index collisions. */
export function deriveArtifactMedia(artifacts: TaskArtifact[]): ArtifactMediaDerived {
  let hasVideo = false;
  const imageUrls: string[] = [];
  const imageCaptions: string[] = [];
  const lightboxKeyOf = new Map<string, number>();
  for (const a of artifacts) {
    const items = a.media ?? [];
    if (items.some((m) => VIDEO_EXTS.includes(m.ext))) hasVideo = true;
    for (const m of items) {
      if (!IMAGE_EXTS.includes(m.ext) || !m.local_path) continue;
      lightboxKeyOf.set(lightboxKey(a.id, m.index), imageUrls.length);
      imageUrls.push(artifactMediaUrl(a.id, m.index));
      imageCaptions.push(m.caption ?? m.filename);
    }
  }
  return { hasVideo, imageUrls, imageCaptions, lightboxKeyOf };
}

/** Build a media URL for a specific artifact media item. */
export function artifactMediaUrl(artifactId: number, mediaIndex: number): string {
  return buildUrl(
    staticRoutePath('getArtifactsByIdMediaByIndex', {
      params: { id: artifactId, index: mediaIndex },
    }),
  );
}
