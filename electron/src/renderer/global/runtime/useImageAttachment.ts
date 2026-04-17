import { useState, useCallback, useRef } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';

export interface ImageAttachment {
  image: File | null;
  preview: string | null;
  setImageFile: (file: File) => void;
  removeImage: () => void;
}

export function useImageAttachment(resetKey?: number | string): ImageAttachment {
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const previewRef = useRef(preview);
  previewRef.current = preview;

  useMountEffect(() => {
    return () => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    };
  });

  // Reset when resetKey changes (e.g. active task ID).
  const prevKeyRef = useRef(resetKey);
  if (resetKey !== undefined && prevKeyRef.current !== resetKey) {
    prevKeyRef.current = resetKey;
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    if (image) setImage(null);
    if (preview) setPreview(null);
    previewRef.current = null;
  }

  const setImageFile = useCallback((file: File) => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    const url = URL.createObjectURL(file);
    setImage(file);
    setPreview(url);
    previewRef.current = url;
  }, []);

  const removeImage = useCallback(() => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    setImage(null);
    setPreview(null);
    previewRef.current = null;
  }, []);

  return { image, preview, setImageFile, removeImage };
}
