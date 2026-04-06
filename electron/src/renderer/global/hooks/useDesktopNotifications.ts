/**
 * Desktop notification hook — processes SSE notification events and
 * shows macOS native notifications via IPC to the main process.
 *
 * Filtering rules:
 * - Critical / High: always show
 * - Normal: only when window is not focused
 * - Low: never show (internal machinery)
 *
 * User can disable all desktop notifications via localStorage preference.
 */
import { useCallback, useRef } from 'react';
import type { NotificationPayload, NotifyLevel, SSEEvent } from '#renderer/types';

const STORAGE_KEY = 'mando:desktop-notifications-enabled';

/** Check if the user has enabled desktop notifications (default: true). */
export function getNotificationsEnabled(): boolean {
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored !== 'false';
}

/** Set the desktop notifications preference. */
export function setNotificationsEnabled(enabled: boolean): void {
  localStorage.setItem(STORAGE_KEY, String(enabled));
}

/** Notification levels that always show regardless of focus. */
const ALWAYS_SHOW_LEVELS: NotifyLevel[] = ['Critical', 'High'];

function shouldShow(level: NotifyLevel, windowFocused: boolean): boolean {
  if (level === 'Low') return false;
  if (ALWAYS_SHOW_LEVELS.includes(level)) return true;
  // Normal: only when unfocused
  return !windowFocused;
}

/**
 * Parse an SSE event into a NotificationPayload, or return null if not a notification.
 * Exported so DataProvider and other consumers share the same structural narrow.
 */
export function parseNotification(event: SSEEvent): NotificationPayload | null {
  if (event.event !== 'notification' || !event.data) return null;

  const data = event.data as Record<string, unknown>;
  if (typeof data.message !== 'string' || typeof data.level !== 'string' || !data.kind) {
    return null;
  }

  return data as unknown as NotificationPayload;
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
    if (!window.mandoAPI?.showNotification) return;

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
      }, 5000);
    }

    window.mandoAPI.showNotification(payload);
  }, []);

  return { processEvent };
}
