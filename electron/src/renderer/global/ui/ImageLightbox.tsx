import React from 'react';
import { useImageLightbox } from '#renderer/global/runtime/useImageLightbox';
import {
  LightboxCaption,
  LightboxCloseButton,
  LightboxCounter,
  LightboxNavButton,
  LightboxZoomIndicator,
} from '#renderer/global/ui/ImageLightboxChrome';

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
  const {
    safeIndex,
    zoom,
    pan,
    dragging,
    imgError,
    hasPrev,
    hasNext,
    imgRef,
    dialogRef,
    navigate,
    onKeyDown,
    onWheel,
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onBackdropClick,
    onImgError,
  } = useImageLightbox({ imageCount: images.length, index, onClose, onNavigate });

  if (images.length === 0) return <></>;

  const multi = images.length > 1;
  const caption = captions?.[safeIndex];

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-label="Image viewer"
      tabIndex={-1}
      className="fixed inset-0 z-[200] flex items-center justify-center"
      style={{ background: 'color-mix(in srgb, var(--background) 95%, transparent)' }}
      onClick={onBackdropClick}
      onKeyDown={onKeyDown}
    >
      <LightboxCloseButton onClose={onClose} />
      {multi && <LightboxCounter index={safeIndex} total={images.length} />}
      {multi && hasPrev && <LightboxNavButton direction="prev" onClick={() => navigate(-1)} />}
      {multi && hasNext && <LightboxNavButton direction="next" onClick={() => navigate(1)} />}
      {zoom > 1 && <LightboxZoomIndicator zoom={zoom} />}

      {imgError ? (
        <div className="flex items-center justify-center rounded-lg bg-muted px-8 py-6 text-[14px] text-text-3">
          Image could not be loaded
        </div>
      ) : (
        <img
          ref={imgRef}
          src={images[safeIndex]}
          alt={caption ?? ''}
          draggable={false}
          className="select-none"
          style={{
            maxWidth: '90vw',
            maxHeight: '90vh',
            objectFit: 'contain',
            transform: `scale(${zoom}) translate(${pan.x / zoom}px, ${pan.y / zoom}px)`,
            cursor: zoom > 1 ? (dragging ? 'grabbing' : 'grab') : 'default',
            transition: dragging ? 'none' : 'transform 0.15s ease-out',
          }}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onError={onImgError}
        />
      )}

      {caption && zoom <= 1 && <LightboxCaption text={caption} />}
    </div>
  );
}
