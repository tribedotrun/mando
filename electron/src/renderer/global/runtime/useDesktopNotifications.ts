/**
 * Desktop notification hook -- processes SSE notification events and
 * shows macOS native notifications via IPC to the main process.
 *
 * Filtering rules:
 * - Critical / High: always show
 * - Normal: only when window is not focused
 * - Low: never show (internal machinery)
 *
 * User can disable all desktop notifications via the typed persistence
 * preference key.
 */
import { useCallback, useRef, useState } from 'react';
import { z } from 'zod';
import type { NotifyLevel, SSEEvent } from '#renderer/global/types';
import { defineJsonSlot } from '#renderer/global/providers/persistence';
import { showNativeNotification } from '#renderer/global/providers/native/notifications';
import { parseNotification } from '#renderer/global/service/notificationHelpers';

const enabledSlot = defineJsonSlot(
  'mando:desktop-notifications-enabled',
  z.boolean(),
  'global/runtime/useDesktopNotifications',
);

const DEDUP_WINDOW_MS = 5000;

/** Check if the user has enabled desktop notifications (default: true). */
export function getNotificationsEnabled(): boolean {
  return enabledSlot.read() ?? true;
}

/** Set the desktop notifications preference. */
export function setNotificationsEnabled(enabled: boolean): void {
  enabledSlot.write(enabled);
}

/** Hook that owns the notification preference state. */
export function useNotificationsPref() {
  const [enabled, setEnabled] = useState(getNotificationsEnabled);
  const toggle = useCallback(() => {
    const next = !enabled;
    setNotificationsEnabled(next);
    setEnabled(next);
  }, [enabled]);
  return { enabled, toggle } as const;
}

/** Notification levels that always show regardless of focus. */
const ALWAYS_SHOW_LEVELS: readonly NotifyLevel[] = Object.freeze(['Critical', 'High'] as const);

function shouldShow(level: NotifyLevel, windowFocused: boolean): boolean {
  if (level === 'Low') return false;
  if (ALWAYS_SHOW_LEVELS.includes(level)) return true;
  // Normal: only when unfocused
  return !windowFocused;
}

/**
 * Hook that processes SSE events and dispatches desktop notifications.
 *
 * Call `processEvent` from the SSE callback for every event received.
 * The hook handles filtering, deduplication, and IPC dispatch.
 */
export function useDesktopNotifications(): {
  processEvent: (event: SSEEvent) => void;
} {
  const recentKeysRef = useRef<Set<string>>(new Set());

  const processEvent = useCallback((event: SSEEvent) => {
    if (!getNotificationsEnabled()) return;

    const payload = parseNotification(event);
    if (!payload) return;

    const windowFocused = document.hasFocus();
    if (!shouldShow(payload.level, windowFocused)) return;

    // Deduplicate by task_key within a short window.
    const taskKey = payload.task_key;
    if (taskKey) {
      if (recentKeysRef.current.has(taskKey)) return;
      recentKeysRef.current.add(taskKey);
      setTimeout(() => {
        recentKeysRef.current.delete(taskKey);
      }, DEDUP_WINDOW_MS);
    }

    showNativeNotification(payload);
  }, []);

  return { processEvent };
}
