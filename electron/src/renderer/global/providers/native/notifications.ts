import type { NotificationKind, NotificationPayload } from '#shared/notifications';

export function showNativeNotification(payload: NotificationPayload): void {
  window.mandoAPI.showNotification(payload);
}

export function subscribeNotificationClick(
  callback: (data: { kind: NotificationKind; item_id?: string }) => void,
): () => void {
  return window.mandoAPI.onNotificationClick(callback);
}
