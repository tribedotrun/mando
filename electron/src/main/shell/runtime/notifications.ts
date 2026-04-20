/**
 * macOS desktop notification handler.
 *
 * Receives notification payloads from the renderer via IPC,
 * shows native macOS notifications, and routes click actions
 * back to the renderer.
 */
import { Notification, BrowserWindow } from 'electron';
import { onChannel, sendChannel } from '#main/global/runtime/ipcSecurity';
import { titleForKind, stripHtml } from '#main/shell/service/notifications';

/**
 * Register IPC handlers for desktop notifications.
 * Call once during app startup. Payload is parsed against the IPC contract schema
 * before reaching the handler, so a malformed renderer message is dropped at the
 * boundary rather than triggering arbitrary native notification behaviour.
 */
export function registerNotificationHandlers(getMainWindow: () => BrowserWindow | null): void {
  onChannel('show-notification', (_event, payload) => {
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
        sendChannel(win.webContents, 'notification-click', {
          kind: payload.kind,
          item_id: itemId,
        });
      }
    });

    notification.show();
  });
}
