import { type ChangeEvent, useCallback, useRef, useState } from 'react';

export function useQAComposer(
  onAsk: (question: string, images?: File[]) => void,
  pending: boolean,
  scrollToBottom: () => void,
) {
  const [question, setQuestion] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const resetComposer = useCallback(() => {
    setQuestion('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, []);

  const submit = useCallback(
    (images?: File[]) => {
      const trimmedQuestion = question.trim();
      if (!trimmedQuestion || pending) return false;
      onAsk(trimmedQuestion, images);
      resetComposer();
      scrollToBottom();
      return true;
    },
    [onAsk, pending, question, resetComposer, scrollToBottom],
  );

  const handleChange = useCallback((event: ChangeEvent<HTMLTextAreaElement>) => {
    setQuestion(event.target.value);
    event.target.style.height = 'auto';
    event.target.style.height = event.target.scrollHeight + 'px';
  }, []);

  return { question, textareaRef, handleChange, submit };
}
