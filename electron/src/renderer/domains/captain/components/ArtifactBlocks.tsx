import React, { useState } from 'react';
import { buildUrl } from '#renderer/global/hooks/useApi';
import { PrMarkdown } from '#renderer/domains/captain/components/PrMarkdown';
import type { TaskArtifact } from '#renderer/types';
import { FileText, Image, ChevronDown, ChevronRight } from 'lucide-react';
import { ImageLightbox } from '#renderer/domains/captain/components/ImageLightbox';

const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp'];

export function EvidenceBlock({
  artifact,
  initialExpanded = false,
}: {
  artifact: TaskArtifact;
  initialExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const time = new Date(artifact.created_at).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
  const mediaCount = artifact.media?.length ?? 0;

  const imageMedia = (artifact.media ?? []).filter(
    (m) => IMAGE_EXTS.includes(m.ext) && m.local_path,
  );
  const imageUrls = imageMedia.map((m) =>
    buildUrl(`/api/artifacts/${artifact.id}/media/${m.index}`),
  );
  const imageCaptions = imageMedia.map((m) => m.caption ?? m.filename);
  const lightboxIndexOf = new Map(imageMedia.map((m, i) => [m.index, i]));

  return (
    <div className="mx-3 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <Image size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body-sm font-medium text-text-1">Evidence</span>
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
            const mediaUrl = buildUrl(`/api/artifacts/${artifact.id}/media/${m.index}`);
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

export function WorkSummaryBlock({
  artifact,
  initialExpanded = false,
}: {
  artifact: TaskArtifact;
  initialExpanded?: boolean;
}) {
  const [expanded, setExpanded] = useState(initialExpanded);
  const time = new Date(artifact.created_at).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });

  return (
    <div className="mx-3 my-2 rounded-lg border border-border bg-surface-1 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-3 text-left"
      >
        <FileText size={16} className="flex-shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="text-body-sm font-medium text-text-1">Work Summary</span>
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
        <div className="mt-3 text-body-sm text-text-1">
          <PrMarkdown text={artifact.content} />
        </div>
      )}
    </div>
  );
}
