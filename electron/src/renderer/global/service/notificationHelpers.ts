import type { NotificationPayload, SSEEvent } from '#renderer/global/types';

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
