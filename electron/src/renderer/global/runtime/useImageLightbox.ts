import {
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type WheelEvent,
  useCallback,
  useRef,
  useState,
} from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { clampZoom } from '#renderer/global/service/lightboxHelpers';

const ZOOM_STEP = 0.3;

interface Args {
  imageCount: number;
  index: number;
  onClose: () => void;
  onNavigate: (index: number) => void;
}

export function useImageLightbox({ imageCount, index, onClose, onNavigate }: Args) {
  const safeIndex = imageCount === 0 ? 0 : Math.min(Math.max(index, 0), imageCount - 1);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const [imgError, setImgError] = useState(false);
  const dragStart = useRef({ x: 0, y: 0 });
  const panStart = useRef({ x: 0, y: 0 });
  const panRef = useRef({ x: 0, y: 0 });
  const imgRef = useRef<HTMLImageElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  const resetView = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
    panRef.current = { x: 0, y: 0 };
    setImgError(false);
  }, []);

  const navigate = useCallback(
    (dir: -1 | 1) => {
      const next = safeIndex + dir;
      if (next >= 0 && next < imageCount) {
        resetView();
        onNavigate(next);
      }
    },
    [safeIndex, imageCount, onNavigate, resetView],
  );

  useMountEffect(() => {
    dialogRef.current?.focus();
  });

  const onKeyDown = useCallback(
    (e: KeyboardEvent) => {
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

  const onWheel = useCallback((e: WheelEvent) => {
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

  const onPointerDown = useCallback(
    (e: PointerEvent) => {
      if (zoom <= 1) return;
      e.preventDefault();
      setDragging(true);
      dragStart.current = { x: e.clientX, y: e.clientY };
      panStart.current = { ...panRef.current };
      imgRef.current?.setPointerCapture(e.pointerId);
    },
    [zoom],
  );

  const onPointerMove = useCallback(
    (e: PointerEvent) => {
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

  const onPointerUp = useCallback(() => setDragging(false), []);

  const onBackdropClick = useCallback(
    (e: MouseEvent) => {
      if (e.target === e.currentTarget) onClose();
    },
    [onClose],
  );

  const hasPrev = safeIndex > 0;
  const hasNext = safeIndex < imageCount - 1;

  return {
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
    onImgError: () => setImgError(true),
  };
}
