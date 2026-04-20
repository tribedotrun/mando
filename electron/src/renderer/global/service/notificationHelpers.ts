import type { NotificationPayload, SSEEvent } from '#renderer/global/types';

/**
 * Parse an SSE event into a NotificationPayload, or return null if not a notification.
 * Exported so DataProvider and other consumers share the same structural narrow.
 */
export function parseNotification(event: SSEEvent): NotificationPayload | null {
  if (event.event !== 'notification') return null;
  return event.data.data ?? null;
}
