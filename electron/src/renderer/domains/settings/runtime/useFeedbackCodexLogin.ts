import { useCallback, useRef, useState } from 'react';
import { toast } from '#renderer/global/runtime/useFeedback';
import { openExternalUrl } from '#renderer/global/providers/native/shell';
import log from '#renderer/global/service/logger';
import {
  useCodexLoginCancel,
  useCodexLoginStart,
  useCodexLoginStatus,
  type CodexLoginFlowInfo,
} from '#renderer/domains/settings/repo/credentialsCodex';

export interface UseFeedbackCodexLoginResult {
  flow: CodexLoginFlowInfo | null;
  actions: {
    start: (opts?: { credentialId?: number }) => void;
    cancel: () => void;
    openAuthUrl: () => void;
  };
}

/**
 * Orchestrates the daemon-driven browser `codex login` flow for one caller
 * (an add-form button or a single credential row's re-login button). Each
 * instance polls only while a flow it started is in flight, and toasts only
 * for the loginId it started (`startedLoginIdRef`), so a flow that another
 * instance cancels-and-replaces produces no cross-talk feedback here. The
 * daemon flow survives even if this hook unmounts mid-flight; the
 * `credentials` SSE event still lands the resulting credential via the
 * shared list query.
 */
export function useFeedbackCodexLogin(): UseFeedbackCodexLoginResult {
  const [polling, setPolling] = useState(false);
  const startMut = useCodexLoginStart();
  const cancelMut = useCodexLoginCancel();
  const statusQuery = useCodexLoginStatus(polling);
  const flow = statusQuery.data?.flow ?? null;
  const startedLoginIdRef = useRef<string | null>(null);
  const startedAtRef = useRef(0);
  const notifiedLoginIdRef = useRef<string | null>(null);

  // Derived state: while polling, react to the daemon's current flow. React
  // permits same-component setState during render for this "store previous
  // value" pattern (see useScoutPage/useSidebarProjectItem for the same
  // shape); the ref mutations are synchronous, so the double render pass
  // cannot re-enter. While startedLoginIdRef is still null the start
  // response is in flight, so polling continues untouched. Toasts are
  // deferred to a microtask because sonner's toast() updates the Toaster
  // component's state, which must not happen during this render; the flow
  // fields are captured into locals first so the microtask does not close
  // over mutable state.
  if (polling && flow && startedLoginIdRef.current !== null) {
    const ownsFlow = flow.loginId === startedLoginIdRef.current;
    const terminal = flow.status !== 'pending';
    if (ownsFlow && terminal) {
      setPolling(false);
      if (notifiedLoginIdRef.current !== flow.loginId) {
        notifiedLoginIdRef.current = flow.loginId;
        const { status, label, error } = flow;
        const warningMessage = flow.warning?.message ?? null;
        queueMicrotask(() => {
          if (status === 'success') {
            toast.success(`Codex account signed in: ${label ?? ''}`);
            if (warningMessage) {
              toast.warning(warningMessage);
            }
          } else if (status === 'failed') {
            toast.error(error ?? 'Codex sign-in failed');
          }
        });
      }
    } else if (!ownsFlow && !terminal && statusQuery.dataUpdatedAt > startedAtRef.current) {
      // A foreign pending flow observed AFTER our start succeeded means
      // another instance started a new flow and the daemon cancelled and
      // replaced ours: stop polling without toasting. Foreign flows in
      // observations from before our start (the cache can still hold the
      // pre-start snapshot right after onSuccess) and foreign terminal
      // flows are stale reads; keep polling until our flow appears.
      setPolling(false);
    }
  }

  const start = useCallback(
    (opts?: { credentialId?: number }) => {
      startedLoginIdRef.current = null;
      notifiedLoginIdRef.current = null;
      setPolling(true);
      startMut.mutate(
        { label: null, credentialId: opts?.credentialId ?? null },
        {
          onSuccess: (res) => {
            // The repo mutation invalidates the status query here, so the
            // next observation with dataUpdatedAt newer than this timestamp
            // reflects the post-start daemon state.
            startedLoginIdRef.current = res.loginId;
            startedAtRef.current = Date.now();
          },
          onError: (err) => {
            setPolling(false);
            toast.error(err.message ?? 'Failed to start browser sign-in');
          },
        },
      );
    },
    [startMut],
  );

  const cancel = useCallback(() => {
    cancelMut.mutate();
  }, [cancelMut]);

  const openAuthUrl = useCallback(() => {
    const url = flow?.authUrl;
    if (!url) return;

    const open = async (): Promise<void> => {
      try {
        await openExternalUrl(url);
      } catch (err: unknown) {
        log.warn('[CodexLogin] openExternalUrl failed', {
          err: err instanceof Error ? err.message : String(err),
        });
      }
    };
    void open();
  }, [flow]);

  return {
    flow,
    actions: { start, cancel, openAuthUrl },
  };
}
