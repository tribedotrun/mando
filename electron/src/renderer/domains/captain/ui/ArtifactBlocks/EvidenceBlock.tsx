import React, { useState } from 'react';
import { formatEventTime } from '#renderer/domains/captain/service/feedHelpers';
import {
  deriveArtifactMedia,
  artifactMediaUrl,
  IMAGE_EXTS,
  VIDEO_EXTS,
} from '#renderer/domains/captain/runtime/artifactHelpers';
import type { TaskArtifact } from '#renderer/global/types';
import { Image, Video, ChevronDown, ChevronRight } from 'lucide-react';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';

export function EvidenceBlock({
  artifact,
  initialExpanded = false,
}: {
  artifact: TaskArtifact;
  initialExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const time = formatEventTime(artifact.created_at);
  const mediaCount = artifact.media?.length ?? 0;

  const { hasVideo, imageUrls, imageCaptions, lightboxIndexOf } = deriveArtifactMedia(artifact);
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
          {artifact.media?.map((m) => {
            const isImage = IMAGE_EXTS.includes(m.ext);
            const isVideo = VIDEO_EXTS.includes(m.ext);
            const mediaUrl = artifactMediaUrl(artifact.id, m.index);
            const lbIdx = lightboxIndexOf.get(m.index);
            return (
              <div key={m.index}>
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
                {(m.caption || m.filename) && (
                  <p className="mt-1 text-caption text-text-3">{m.caption ?? m.filename}</p>
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
