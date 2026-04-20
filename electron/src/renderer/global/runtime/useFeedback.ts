// Canonical feedback adapter -- the single non-ui module allowed to import
// `toast` from `sonner`. Per R9, every toast in non-ui code must funnel
// through here so repo/runtime/service tiers depend on one abstraction
// instead of a third-party API. UI code may still import `sonner` directly.
//
// Re-export is intentional: keeping the `toast` name and shape minimizes
// migration churn and preserves sonner's full API (success/error/info/
// warning/loading/promise/dismiss/custom). Future behavioral centralization
// (rate limiting, analytics, a11y hooks) lives here.

import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/global/service/utils';
import log from '#renderer/global/service/logger';

export { toast };

/**
 * Write text to the clipboard and surface a success/error toast via the
 * canonical feedback adapter. Returns true on success so callers can decide
 * whether to run follow-up logic.
 */
// invariant: clipboard result is encoded as boolean; errors are surfaced via toast + false return, not propagated
export async function copyToClipboard(text: string, label?: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    if (label) {
      toast.success(label);
    }
    return true;
  } catch (err) {
    log.warn('clipboard write failed:', err);
    toast.error(getErrorMessage(err, 'Copy failed, clipboard access denied'));
    return false;
  }
}
