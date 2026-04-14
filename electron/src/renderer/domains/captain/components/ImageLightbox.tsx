import React, { useCallback, useRef, useState } from 'react';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { Button } from '#renderer/components/ui/button';

interface Props {
  images: string[];
  index: number;
  captions?: string[];
  onClose: () => void;
  onNavigate: (index: number) => void;
}

const MIN_ZOOM = 1;
const MAX_ZOOM = 5;
const ZOOM_STEP = 0.3;

function clampZoom(value: number): number {
  return value < MIN_ZOOM ? MIN_ZOOM : value > MAX_ZOOM ? MAX_ZOOM : value;
}

function formatZoomPercent(zoom: number): string {
  return `${(zoom * 100) | 0}%`;
}

const CONTROL_BG = 'color-mix(in srgb, var(--foreground) 10%, transparent)';

export function ImageLightbox({
  images,
  index,
  captions,
  onClose,
  onNavigate,
}: Props): React.ReactElement {
  const safeIndex =
    images.length === 0 ? 0 : index < 0 ? 0 : index >= images.length ? images.length - 1 : index;
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const [imgError, setImgError] = useState(false);
  const dragStart = useRef({ x: 0, y: 0 });
  const panStart = useRef({ x: 0, y: 0 });
  const panRef = useRef({ x: 0, y: 0 });
  const imgRef = useRef<HTMLImageElement>(null);

  const resetView = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
    panRef.current = { x: 0, y: 0 };
    setImgError(false);
  }, []);

  const navigate = useCallback(
    (dir: -1 | 1) => {
      const next = safeIndex + dir;
      if (next >= 0 && next < images.length) {
        resetView();
        onNavigate(next);
      }
    },
    [safeIndex, images.length, onNavigate, resetView],
  );

  const dialogRef = useRef<HTMLDivElement>(null);

  useMountEffect(() => {
    dialogRef.current?.focus();
  });

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      e.stopPropagation();
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        navigate(-1);
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        navigate(1);
      }
    },
    [navigate, onClose],
  );

  const handleWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    e.stopPropagation();
    const delta = e.deltaY < 0 ? ZOOM_STEP : -ZOOM_STEP;
    setZoom((prev) => {
      const next = clampZoom(prev + delta);
      if (next === 1) {
        setPan({ x: 0, y: 0 });
        panRef.current = { x: 0, y: 0 };
      }
      return next;
    });
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (zoom <= 1) return;
      e.preventDefault();
      setDragging(true);
      dragStart.current = { x: e.clientX, y: e.clientY };
      panStart.current = { ...panRef.current };
      imgRef.current?.setPointerCapture(e.pointerId);
    },
    [zoom],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!dragging) return;
      const next = {
        x: panStart.current.x + (e.clientX - dragStart.current.x),
        y: panStart.current.y + (e.clientY - dragStart.current.y),
      };
      panRef.current = next;
      setPan(next);
    },
    [dragging],
  );

  const handlePointerUp = useCallback(() => {
    setDragging(false);
  }, []);

  const handleBackdropClick = useCallback(
    (e: React.MouseEvent) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose],
  );

  if (images.length === 0) return <></>;

  const multi = images.length > 1;
  const hasPrev = safeIndex > 0;
  const hasNext = safeIndex < images.length - 1;

  return (
    <div
      ref={dialogRef}
      role="dialog"
      aria-modal="true"
      aria-label="Image viewer"
      tabIndex={-1}
      className="fixed inset-0 z-[200] flex items-center justify-center"
      style={{ background: 'color-mix(in srgb, var(--background) 95%, transparent)' }}
      onClick={handleBackdropClick}
      onKeyDown={handleKeyDown}
    >
      {/* Close button */}
      <Button
        variant="ghost"
        size="icon-sm"
        onClick={onClose}
        className="fixed right-4 top-4 z-[201] rounded-full text-foreground"
        style={{ background: CONTROL_BG }}
        aria-label="Close"
      >
        <X size={14} strokeWidth={2} />
      </Button>

      {/* Counter */}
      {multi && (
        <div
          className="fixed left-1/2 top-4 z-[201] -translate-x-1/2 rounded-full px-3 py-1 text-[12px] text-muted-foreground"
          style={{ background: CONTROL_BG }}
        >
          {safeIndex + 1} / {images.length}
        </div>
      )}

      {/* Prev button */}
      {multi && hasPrev && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => navigate(-1)}
          className="fixed left-4 top-1/2 z-[201] -translate-y-1/2 rounded-full text-foreground"
          style={{ background: CONTROL_BG }}
          aria-label="Previous image"
        >
          <ChevronLeft size={16} strokeWidth={2} />
        </Button>
      )}

      {/* Next button */}
      {multi && hasNext && (
        <Button
          variant="ghost"
          size="icon"
          onClick={() => navigate(1)}
          className="fixed right-4 top-1/2 z-[201] -translate-y-1/2 rounded-full text-foreground"
          style={{ background: CONTROL_BG }}
          aria-label="Next image"
        >
          <ChevronRight size={16} strokeWidth={2} />
        </Button>
      )}

      {/* Zoom indicator */}
      {zoom > 1 && (
        <div
          className="fixed bottom-4 left-1/2 z-[201] -translate-x-1/2 rounded-full px-3 py-1 text-[12px] text-muted-foreground"
          style={{ background: CONTROL_BG }}
        >
          {formatZoomPercent(zoom)}
        </div>
      )}

      {/* Image */}
      {imgError ? (
        <div className="flex items-center justify-center rounded-lg bg-muted px-8 py-6 text-[14px] text-text-3">
          Image could not be loaded
        </div>
      ) : (
        <img
          ref={imgRef}
          src={images[safeIndex]}
          alt={captions?.[safeIndex] ?? ''}
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
          onWheel={handleWheel}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onError={() => setImgError(true)}
        />
      )}

      {/* Caption */}
      {captions?.[safeIndex] && zoom <= 1 && (
        <div
          className="fixed bottom-4 left-1/2 z-[201] -translate-x-1/2 rounded-full px-4 py-1.5 text-[13px] text-foreground"
          style={{ background: CONTROL_BG }}
        >
          {captions[safeIndex]}
        </div>
      )}
    </div>
  );
}
