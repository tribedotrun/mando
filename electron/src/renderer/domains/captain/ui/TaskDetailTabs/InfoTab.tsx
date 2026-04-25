import React, { useState } from 'react';
import type { TaskItem } from '#renderer/global/types';
import { shortenPath } from '#renderer/global/service/utils';
import { taskImageUrl } from '#renderer/global/runtime/useApi';
import { ImageLightbox } from '#renderer/global/ui/ImageLightbox';
import { CopyValue } from '#renderer/domains/captain/ui/CopyValue';

export function InfoTab({ item }: { item: TaskItem }): React.ReactElement {
  const [lightbox, setLightbox] = useState<{ images: string[]; index: number } | null>(null);
  const imageFilenames =
    item.images
      ?.split(',')
      .map((s) => s.trim())
      .filter(Boolean) ?? [];
  const imageUrls = imageFilenames.map(taskImageUrl);

  return (
    <div className="space-y-5">
      <div className="grid grid-cols-[auto_1fr] items-baseline gap-x-6 gap-y-2.5">
        <span className="text-caption text-text-4">ID</span>
        <span className="font-mono text-caption text-text-2">#{item.id}</span>

        {item.worktree && (
          <>
            <span className="text-caption text-text-4">Worktree</span>
            <CopyValue value={item.worktree} display={shortenPath(item.worktree)} />
          </>
        )}

        {item.branch && (
          <>
            <span className="text-caption text-text-4">Branch</span>
            <CopyValue value={item.branch} />
          </>
        )}

        {item.plan && (
          <>
            <span className="text-caption text-text-4">Plan</span>
            <CopyValue value={item.plan} display={shortenPath(item.plan)} />
          </>
        )}

        {item.no_auto_merge && (
          <>
            <span className="text-caption text-text-4">Auto-merge</span>
            <span className="text-caption text-text-2">Disabled</span>
          </>
        )}

        {item.planning && (
          <>
            <span className="text-caption text-text-4">Mode</span>
            <span className="text-caption text-text-2">Plan</span>
          </>
        )}
      </div>

      {(item.original_prompt || imageUrls.length > 0) && (
        <div>
          <div className="mb-1.5 text-caption text-text-4">Original Request</div>
          {item.original_prompt && (
            <p className="text-body leading-relaxed text-text-2 [overflow-wrap:anywhere]">
              {item.original_prompt}
            </p>
          )}
          {imageUrls.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-2">
              {imageUrls.map((src, i) => (
                <img
                  key={imageFilenames[i]}
                  src={src}
                  alt={imageFilenames[i]}
                  className="max-h-64 cursor-pointer rounded border border-border object-contain transition-opacity hover:opacity-80"
                  onClick={() => setLightbox({ images: imageUrls, index: i })}
                />
              ))}
            </div>
          )}
        </div>
      )}

      {lightbox && (
        <ImageLightbox
          images={lightbox.images}
          index={lightbox.index}
          onClose={() => setLightbox(null)}
          onNavigate={(i) => setLightbox((prev) => (prev ? { ...prev, index: i } : null))}
        />
      )}
    </div>
  );
}
