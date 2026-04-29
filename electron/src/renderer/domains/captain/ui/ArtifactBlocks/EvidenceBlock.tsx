import React, { useMemo, useState } from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import {
  artifactMediaUrl,
  deriveArtifactMedia,
  flattenArtifactMedia,
  IMAGE_EXTS,
  lightboxKey,
  summarizeArtifactGroup,
  VIDEO_EXTS,
} from '#renderer/domains/captain/runtime/artifactHelpers';
import type { TaskArtifact } from '#renderer/global/types';
import { Image, Video, ChevronDown, ChevronRight } from 'lucide-react';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';

export function EvidenceBlock({
  artifacts,
  initialExpanded = false,
}: {
  artifacts: TaskArtifact[];
  initialExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);

  const { mediaCount, latestTimestamp, hasVideo } = useMemo(
    () => summarizeArtifactGroup(artifacts),
    [artifacts],
  );
  const { imageUrls, imageCaptions, lightboxKeyOf } = useMemo(
    () => deriveArtifactMedia(artifacts),
    [artifacts],
  );
  const flatMedia = useMemo(() => flattenArtifactMedia(artifacts), [artifacts]);
  const time = formatEventTime(latestTimestamp);
  const EvidenceIcon = hasVideo ? Video : Image;

  return (
    <div className="mx-3 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <EvidenceIcon size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body font-medium text-text-1">Evidence</span>
            <span className="text-caption text-text-3">
              {mediaCount} {mediaCount === 1 ? 'file' : 'files'}
            </span>
            <span className="text-caption text-text-3">{time}</span>
          </div>
        </div>
        {expanded ? (
          <ChevronDown size={14} className="text-text-3" />
        ) : (
          <ChevronRight size={14} className="text-text-3" />
        )}
      </button>
      {expanded && (
        <div className="mt-3 space-y-3">
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
                      <span className="rounded bg-secondary px-1.5 text-[10px] font-medium uppercase tracking-wide text-text-2">
                        before
                      </span>
                    )}
                    {m.kind === 'after_fix' && (
                      <span
                        className="rounded px-1.5 text-[10px] font-medium uppercase tracking-wide"
                        style={{
                          backgroundColor: 'var(--success-bg)',
                          color: 'var(--success)',
                        }}
                      >
                        after
                      </span>
                    )}
                    {m.kind === 'cannot_reproduce' && (
                      <span className="rounded bg-secondary px-1.5 text-[10px] font-medium uppercase tracking-wide text-text-2">
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
      )}
      {lightbox && (
        <ImageLightbox
          images={lightbox.images}
          index={lightbox.index}
          captions={imageCaptions}
          onClose={() => setLightbox(null)}
          onNavigate={(i) => setLightbox((prev) => (prev ? { ...prev, index: i } : null))}
        />
      )}
    </div>
  );
}
