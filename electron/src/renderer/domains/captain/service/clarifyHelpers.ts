import type { ClarifierQuestion } from '#renderer/global/types';

/** Filters questions that haven't been self-answered by the AI. */
export function getUnansweredQuestions(questions: ClarifierQuestion[]): ClarifierQuestion[] {
  return questions.filter((q) => !q.self_answered);
}

/** Builds a fingerprint string for draft key derivation. */
export function clarifyFingerprint(unanswered: ClarifierQuestion[]): string {
  return `${unanswered.length}:${unanswered
    .map((q) => q.question)
    .join('|')
    .slice(0, 64)}`;
}

/** Assembles the submit payload from unanswered questions and user answers. */
export function buildClarifyPayload(
  unanswered: ClarifierQuestion[],
  answers: Record<number, string>,
): { question: string; answer: string }[] {
  return unanswered
    .map((q, i) => ({ question: q.question, answer: answers[i]?.trim() || '' }))
    .filter((a) => a.answer.length > 0);
}

/** Counts how many questions have non-empty answers. */
export function filledAnswerCount(
  unanswered: ClarifierQuestion[],
  answers: Record<number, string>,
): number {
  return unanswered.filter((_, i) => answers[i]?.trim()).length;
}
