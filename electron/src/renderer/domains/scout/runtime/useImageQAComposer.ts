import { type ChangeEvent, useCallback, useRef } from 'react';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';

export function useImageQAComposer(
  onAsk: (question: string, images?: File[]) => void,
  pending: boolean,
  scrollRef: React.MutableRefObject<(() => void) | null>,
  draftKey: string,
) {
  const { text, setText, image, preview, setImageFile, removeImage, clearDraft } =
    useTextImageDraft(draftKey);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  const scrollToBottom = useCallback(() => scrollRef.current?.(), [scrollRef]);

  const handleChange = useCallback(
    (event: ChangeEvent<HTMLTextAreaElement>) => {
      setText(event.target.value);
      event.target.style.height = 'auto';
      event.target.style.height = event.target.scrollHeight + 'px';
    },
    [setText],
  );

  const doSubmit = useCallback(() => {
    const trimmed = text.trim();
    if (!trimmed || pending) return;
    const images = image ? [image] : undefined;
    onAsk(trimmed, images);
    clearDraft();
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
    scrollToBottom();
  }, [clearDraft, image, onAsk, pending, scrollToBottom, text]);

  return {
    text: { question: text, textareaRef, handleChange },
    submit: { doSubmit, pending },
    image: { image, preview, setImageFile, removeImage, fileRef },
  };
}
