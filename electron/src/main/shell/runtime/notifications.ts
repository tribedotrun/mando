/**
 * macOS desktop notification handler.
 *
 * Receives notification payloads from the renderer via IPC,
 * shows native macOS notifications, and routes click actions
 * back to the renderer.
 */
import { Notification, BrowserWindow } from 'electron';
import { onTrusted } from '#main/global/runtime/ipcSecurity';
import { titleForKind, stripHtml } from '#main/shell/service/notifications';
import type { NotificationPayload } from '#shared/notifications';

/**
 * Register IPC handlers for desktop notifications.
 * Call once during app startup.
 */
export function registerNotificationHandlers(getMainWindow: () => BrowserWindow | null): void {
  onTrusted('show-notification', (_event, payload: NotificationPayload) => {
    if (!Notification.isSupported()) return;

    const title = titleForKind(payload.kind);
    const body = stripHtml(payload.message);

    const notification = new Notification({
      title,
      body,
      silent: payload.level === 'Low' || payload.level === 'Normal',
      urgency: payload.level === 'Critical' ? 'critical' : 'normal',
    });

    notification.on('click', () => {
      const win = getMainWindow();
      if (win) {
        win.show();
        win.focus();
        const itemId =
          'item_id' in payload.kind
            ? payload.kind.item_id
            : 'scout_id' in payload.kind
              ? String(payload.kind.scout_id)
              : undefined;
        win.webContents.send('notification-click', {
          kind: payload.kind,
          item_id: itemId,
        });
      }
    });

    notification.show();
  });
}
