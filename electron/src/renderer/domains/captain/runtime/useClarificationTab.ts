import { useCallback } from 'react';
import { toast } from '#renderer/global/runtime/useFeedback';
import type { ClarifierQuestion } from '#renderer/global/types';
import { useTaskClarify } from '#renderer/domains/captain/runtime/hooks';
import { useDraftRecord } from '#renderer/domains/captain/runtime/useDraft';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import {
  buildClarifyPayload,
  clarifyFingerprint,
  filledAnswerCount,
  getUnansweredQuestions,
} from '#renderer/domains/captain/service/clarifyHelpers';
import { clarifyResultToToast } from '#renderer/global/service/utils';

export function useClarificationTab(taskId: number, questions: ClarifierQuestion[]) {
  const unanswered = getUnansweredQuestions(questions);
  const qFingerprint = clarifyFingerprint(unanswered);
  const [answers, setAnswers, clearAnswersDraft] = useDraftRecord(
    `mando:draft:clarify:${taskId}:${qFingerprint}`,
  );
  const clarifyMut = useTaskClarify();
  const { image, preview, setImageFile, removeImage } = useImageAttachment();

  const filledCount = filledAnswerCount(unanswered, answers);

  const handleSubmit = useCallback(async () => {
    const payload = buildClarifyPayload(unanswered, answers);
    if (payload.length === 0) return null;

    try {
      const images = image ? [image] : undefined;
      const result = await clarifyMut.mutateAsync({
        id: taskId,
        mode: 'structured' as const,
        answers: payload,
        images,
      });
      const { variant, msg } = clarifyResultToToast(result.status);
      const fn = variant === 'success' ? toast.success : toast.info;
      fn(msg);
      clearAnswersDraft();
      removeImage();
      if (result.status !== 'clarifying') return msg;
    } catch {
      // toast handled by mutation hook
    }
    return null;
  }, [answers, unanswered, taskId, clearAnswersDraft, image, removeImage, clarifyMut]);

  return {
    unanswered,
    answers,
    setAnswers,
    filledCount,
    clarifyMut,
    image,
    preview,
    setImageFile,
    removeImage,
    handleSubmit,
  };
}
