import React, { useMemo, useState } from 'react';
import {
  artifactMediaUrl,
  deriveArtifactMedia,
  flattenArtifactMedia,
  IMAGE_EXTS,
  lightboxKey,
  VIDEO_EXTS,
} from '#renderer/domains/captain/runtime/artifactHelpers';
import type { TaskArtifact } from '#renderer/global/types';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';

export function EvidenceMediaList({ artifacts }: { artifacts: TaskArtifact[] }) {
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const { imageUrls, imageCaptions, lightboxKeyOf } = useMemo(
    () => deriveArtifactMedia(artifacts),
    [artifacts],
  );
  const flatMedia = useMemo(() => flattenArtifactMedia(artifacts), [artifacts]);

  return (
    <>
      <div className="space-y-3">
        {flatMedia.map(({ artifactId, media: m }) => {
          const isImage = IMAGE_EXTS.includes(m.ext);
          const isVideo = VIDEO_EXTS.includes(m.ext);
          const mediaUrl = artifactMediaUrl(artifactId, m.index);
          const lbIdx = lightboxKeyOf.get(lightboxKey(artifactId, m.index));
          return (
            <div key={lightboxKey(artifactId, m.index)}>
              {isImage && m.local_path && (
                <img
                  src={mediaUrl}
                  alt={m.caption ?? m.filename}
                  className="max-h-64 cursor-pointer rounded border border-border object-contain transition-opacity hover:opacity-80"
                  onClick={() => {
                    if (lbIdx !== undefined) setLightbox({ images: imageUrls, index: lbIdx });
                  }}
                />
              )}
              {isVideo && m.local_path && (
                <video
                  src={mediaUrl}
                  controls
                  muted
                  playsInline
                  preload="metadata"
                  className="max-h-64 w-full rounded border border-border object-contain"
                >
                  <track kind="captions" />
                </video>
              )}
              {(m.caption || m.filename || m.kind) && (
                <div className="mt-1 flex items-baseline gap-2">
                  {m.kind === 'before_fix' && (
                    <span className="rounded bg-secondary px-1.5 text-[11px] font-medium uppercase tracking-wide text-text-2">
                      before
                    </span>
                  )}
                  {m.kind === 'after_fix' && (
                    <span
                      className="rounded px-1.5 text-[11px] font-medium uppercase tracking-wide"
                      style={{
                        backgroundColor: 'var(--success-bg)',
                        color: 'var(--success)',
                      }}
                    >
                      after
                    </span>
                  )}
                  {m.kind === 'cannot_reproduce' && (
                    <span className="rounded bg-secondary px-1.5 text-[11px] font-medium uppercase tracking-wide text-text-2">
                      no repro
                    </span>
                  )}
                  <p className="text-caption text-text-3">{m.caption ?? m.filename}</p>
                </div>
              )}
            </div>
          );
        })}
      </div>
      {lightbox && (
        <ImageLightbox
          images={lightbox.images}
          index={lightbox.index}
          captions={imageCaptions}
          onClose={() => setLightbox(null)}
          onNavigate={(i) => setLightbox((prev) => (prev ? { ...prev, index: i } : null))}
        />
      )}
    </>
  );
}
