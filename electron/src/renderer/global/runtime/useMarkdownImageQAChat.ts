import { useCallback, useRef } from 'react';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import { useQAComposer } from '#renderer/global/runtime/useQAComposer';

export function useMarkdownImageQAChat(
  onAsk: (question: string, images?: File[]) => void,
  pending: boolean,
  scrollRef: React.MutableRefObject<(() => void) | null>,
) {
  const scrollToBottom = useCallback(() => scrollRef.current?.(), [scrollRef]);
  const { question, textareaRef, handleChange, submit } = useQAComposer(
    onAsk,
    pending,
    scrollToBottom,
  );
  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const fileRef = useRef<HTMLInputElement>(null);

  function doSubmit(): void {
    if (submit(image ? [image] : undefined)) removeImage();
  }

  return {
    question,
    textareaRef,
    handleChange,
    doSubmit,
    pending,
    image,
    preview,
    setImageFile,
    removeImage,
    fileRef,
  };
}
