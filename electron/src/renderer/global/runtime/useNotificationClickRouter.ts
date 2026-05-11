import { useRef } from 'react';
import { useNavigate } from '@tanstack/react-router';
import type { NotificationKind } from '#shared/notifications';
import { useNotificationClicks } from '#renderer/global/runtime/useNativeActions';
import { useTaskWorkbenchLookup } from '#renderer/global/runtime/useTaskCacheLookup';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { resolveNotificationClickTarget } from '#renderer/global/runtime/notificationClickRouting';
import log from '#renderer/global/service/logger';

/**
 * Subscribes to desktop notification clicks and routes them by NotificationKind:
 * task kinds → /wb/$workbenchId, scout kinds → /scout?item=<scout_id>,
 * RateLimited → /, Generic → no-op.
 *
 * Also exposes `window.__mandoTestNotificationClick` so per-PR E2E specs can
 * exercise the routing pipeline (helper + navigate) without an OS-level
 * notification click. Mirrors the `__buildComponentMap` / `__devInspectorCopy`
 * pattern in `devInspector` — same global-bridge convention for agent-driven
 * verification.
 */
export function useNotificationClickRouter(): void {
  const navigate = useNavigate();
  const lookupWorkbench = useTaskWorkbenchLookup();

  const navigateRef = useRef(navigate);
  navigateRef.current = navigate;
  const lookupRef = useRef(lookupWorkbench);
  lookupRef.current = lookupWorkbench;

  const dispatch = (kind: NotificationKind): void => {
    const target = resolveNotificationClickTarget(kind, lookupRef.current);
    switch (target.type) {
      case 'workbench':
        void navigateRef.current({
          to: '/wb/$workbenchId',
          params: { workbenchId: String(target.workbenchId) },
        });
        return;
      case 'workbench-miss':
        log.warn('notification click: no workbench for task', { taskId: target.taskId });
        void navigateRef.current({ to: '/' });
        return;
      case 'scout':
        void navigateRef.current({ to: '/scout', search: { item: target.scoutId } });
        return;
      case 'home':
        void navigateRef.current({ to: '/' });
        return;
      case 'noop':
        return;
    }
  };

  const dispatchRef = useRef(dispatch);
  dispatchRef.current = dispatch;

  useNotificationClicks((data) => dispatchRef.current(data.kind));

  useMountEffect(() => {
    window.__mandoTestNotificationClick = (kind: NotificationKind) => dispatchRef.current(kind);
    return () => {
      delete window.__mandoTestNotificationClick;
    };
  });
}
