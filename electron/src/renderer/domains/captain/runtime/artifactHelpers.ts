import { buildUrl } from '#renderer/global/runtime/useApi';
import { staticRoutePath } from '#renderer/global/providers/http';
import type { TaskArtifact } from '#renderer/global/types';

export const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp'];
export const VIDEO_EXTS = ['mp4', 'mov', 'webm'];

export interface ArtifactMediaDerived {
  hasVideo: boolean;
  imageUrls: string[];
  imageCaptions: string[];
  lightboxIndexOf: Map<number, number>;
}

/** Derives display-ready image/video data from an artifact's media array. */
export function deriveArtifactMedia(artifact: TaskArtifact): ArtifactMediaDerived {
  const media = artifact.media ?? [];
  const hasVideo = media.some((m) => VIDEO_EXTS.includes(m.ext));
  const imageMedia = media.filter((m) => IMAGE_EXTS.includes(m.ext) && m.local_path);
  return {
    hasVideo,
    imageUrls: imageMedia.map((m) =>
      buildUrl(
        staticRoutePath('getArtifactsByIdMediaByIndex', {
          params: { id: artifact.id, index: m.index },
        }),
      ),
    ),
    imageCaptions: imageMedia.map((m) => m.caption ?? m.filename),
    lightboxIndexOf: new Map(imageMedia.map((m, i) => [m.index, i])),
  };
}

/** Build a media URL for a specific artifact media item. */
export function artifactMediaUrl(artifactId: number, mediaIndex: number): string {
  return buildUrl(
    staticRoutePath('getArtifactsByIdMediaByIndex', {
      params: { id: artifactId, index: mediaIndex },
    }),
  );
}
