import { useCallback, useState } from 'react';

import log from '#renderer/logger';
import { getErrorMessage } from '#renderer/utils';

export type TGValidationResult = { botUsername?: string; error?: string } | null;

/**
 * Shared Telegram token validation state machine used by the onboarding
 * wizard and the Settings Telegram panel. Both surfaces needed the same
 * validating/result flow plus the same IPC invocation; centralising here
 * prevents drift in the error messages and response-shape handling.
 */
export function useTelegramTokenValidator(): {
  validating: boolean;
  result: TGValidationResult;
  validate: (token: string) => Promise<boolean>;
  reset: () => void;
  setResult: (r: TGValidationResult) => void;
} {
  const [validating, setValidating] = useState(false);
  const [result, setResult] = useState<TGValidationResult>(null);

  const validate = useCallback(async (token: string): Promise<boolean> => {
    const trimmed = token.trim();
    if (!trimmed) {
      setResult({ error: 'Token is required' });
      return false;
    }
    setValidating(true);
    setResult(null);
    try {
      const res = await window.mandoAPI.validateTelegramToken(trimmed);
      if (res.valid) {
        setResult({ botUsername: res.botUsername });
        return true;
      }
      setResult({ error: res.error ?? 'Invalid token' });
      return false;
    } catch (e) {
      log.warn('[TG] validate failed', e);
      setResult({ error: getErrorMessage(e, 'Validation failed, check your network') });
      return false;
    } finally {
      setValidating(false);
    }
  }, []);

  const reset = useCallback(() => {
    setResult(null);
  }, []);

  return { validating, result, validate, reset, setResult };
}
