import log from '#renderer/global/service/logger';

/**
 * Run `result`, clearing the draft only after a resolved Promise (or a sync
 * void return). If `result` rejects, the draft is preserved so the user can
 * retry without retyping.
 */
export async function runAndClearOnSuccess(
  result: Promise<void> | void,
  clearDraft: () => void,
): Promise<void> {
  if (result && typeof (result as Promise<void>).then === 'function') {
    try {
      await result;
      clearDraft();
    } catch (err) {
      log.warn('[PromptModal] submit rejected; preserving draft for retry', { err });
    }
    return;
  }
  clearDraft();
}
