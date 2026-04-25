import React from 'react';
import { useImageLightbox } from '#renderer/global/runtime/useImageLightbox';
import {
  LightboxCaption,
  LightboxCloseButton,
  LightboxCounter,
  LightboxNavButton,
  LightboxZoomIndicator,
} from '#renderer/global/ui/ImageLightboxParts';

interface Props {
  images: string[];
  index: number;
  captions?: string[];
  onClose: () => void;
  onNavigate: (index: number) => void;
}

export function ImageLightbox({
  images,
  index,
  captions,
  onClose,
  onNavigate,
}: Props): React.ReactElement {
  const lightbox = useImageLightbox({ imageCount: images.length, index, onClose, onNavigate });

  if (images.length === 0) return <></>;

  const multi = images.length > 1;
  const caption = captions?.[lightbox.view.safeIndex];

  return (
    <div
      ref={lightbox.refs.dialogRef}
      role="dialog"
      aria-modal="true"
      aria-label="Image viewer"
      tabIndex={-1}
      className="fixed inset-0 z-[200] flex items-center justify-center"
      style={{ background: 'color-mix(in srgb, var(--background) 95%, transparent)' }}
      onClick={lightbox.events.onBackdropClick}
      onKeyDown={lightbox.events.onKeyDown}
    >
      <LightboxCloseButton onClose={onClose} />
      {multi && <LightboxCounter index={lightbox.view.safeIndex} total={images.length} />}
      {multi && lightbox.view.hasPrev && (
        <LightboxNavButton direction="prev" onClick={() => lightbox.actions.navigate(-1)} />
      )}
      {multi && lightbox.view.hasNext && (
        <LightboxNavButton direction="next" onClick={() => lightbox.actions.navigate(1)} />
      )}
      {lightbox.view.zoom > 1 && <LightboxZoomIndicator zoom={lightbox.view.zoom} />}

      {lightbox.view.imgError ? (
        <div className="flex items-center justify-center rounded-lg bg-muted px-8 py-6 text-[14px] text-text-3">
          Image could not be loaded
        </div>
      ) : (
        <img
          ref={lightbox.refs.imgRef}
          src={images[lightbox.view.safeIndex]}
          alt={caption ?? ''}
          draggable={false}
          className="select-none"
          style={{
            maxWidth: '90vw',
            maxHeight: '90vh',
            objectFit: 'contain',
            transform: `scale(${lightbox.view.zoom}) translate(${lightbox.view.pan.x / lightbox.view.zoom}px, ${lightbox.view.pan.y / lightbox.view.zoom}px)`,
            cursor:
              lightbox.view.zoom > 1 ? (lightbox.view.dragging ? 'grabbing' : 'grab') : 'default',
            transition: lightbox.view.dragging ? 'none' : 'transform 0.15s ease-out',
          }}
          onWheel={lightbox.events.onWheel}
          onPointerDown={lightbox.events.onPointerDown}
          onPointerMove={lightbox.events.onPointerMove}
          onPointerUp={lightbox.events.onPointerUp}
          onError={lightbox.actions.onImgError}
        />
      )}

      {caption && lightbox.view.zoom <= 1 && <LightboxCaption text={caption} />}
    </div>
  );
}
