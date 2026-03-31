/**
 * macOS desktop notification handler.
 *
 * Receives notification payloads from the renderer via IPC,
 * shows native macOS notifications, and routes click actions
 * back to the renderer.
 */
import { Notification, shell, BrowserWindow } from 'electron';
import { onTrusted } from '#main/ipc-security';

/** Notification payload shape (matches Rust NotificationPayload). */
interface NotificationPayload {
  message: string;
  level: 'Low' | 'Normal' | 'High' | 'Critical';
  kind: NotificationKind;
  task_key?: string;
  reply_markup?: unknown;
}

type NotificationKind =
  | { type: 'AwaitingReview'; item_id: string; pr_number?: number }
  | { type: 'ClarifierNeeded'; item_id: string }
  | { type: 'RebaseFailed'; item_id: string; pr_number: number }
  | { type: 'WorkerEscalated'; item_id: string }
  | { type: 'CaptainReviewVerdict'; item_id: string; verdict: string; feedback?: string }
  | { type: 'Escalated'; item_id: string; summary?: string }
  | { type: 'Errored'; item_id: string; error?: string }
  | { type: 'NeedsClarification'; item_id: string; questions?: string }
  | { type: 'CronAlert'; action_id: string }
  | {
      type: 'RateLimited';
      status: string;
      utilization?: number;
      resets_at?: number;
      rate_limit_type?: string;
      overage_status?: string;
      overage_resets_at?: number;
      overage_disabled_reason?: string;
    }
  | { type: 'Generic' };

/** Map notification kind to a human-readable title. */
function titleForKind(kind: NotificationKind): string {
  switch (kind.type) {
    case 'AwaitingReview':
      return 'PR Ready for Review';
    case 'ClarifierNeeded':
      return 'Clarification Needed';
    case 'RebaseFailed':
      return 'Rebase Failed';
    case 'WorkerEscalated':
      return 'Worker Escalated';
    case 'CaptainReviewVerdict':
      return 'Captain Review Verdict';
    case 'Escalated':
      return 'Escalated';
    case 'Errored':
      return 'Error';
    case 'NeedsClarification':
      return 'Clarification Needed';
    case 'CronAlert':
      return 'Cron Alert';
    case 'RateLimited':
      return 'Rate Limited';
    case 'Generic':
      return 'Mando';
  }
}

/** Strip HTML tags from message text for native notification body. */
function stripHtml(html: string): string {
  return html.replace(/<[^>]*>/g, '');
}

/** Extract a clickable URL from the notification kind + message. */
function extractUrl(payload: NotificationPayload): string | null {
  // Try to extract a GitHub PR URL from the message HTML.
  const hrefMatch = payload.message.match(/href="(https:\/\/github\.com\/[^"]+)"/);
  if (hrefMatch) return hrefMatch[1];

  // For PR-related kinds, construct URL if we can find repo info in the message.
  const kind = payload.kind;
  if ('pr_number' in kind && kind.pr_number) {
    const repoMatch = payload.message.match(/([a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+)/);
    if (repoMatch) {
      return `https://github.com/${repoMatch[1]}/pull/${kind.pr_number}`;
    }
  }

  return null;
}

/**
 * Register IPC handlers for desktop notifications.
 * Call once during app startup.
 */
export function registerNotificationHandlers(getMainWindow: () => BrowserWindow | null): void {
  onTrusted('show-notification', (_event, payload: NotificationPayload) => {
    if (!Notification.isSupported()) return;

    const title = titleForKind(payload.kind);
    const body = stripHtml(payload.message);
    const clickUrl = extractUrl(payload);

    const notification = new Notification({
      title,
      body,
      silent: payload.level === 'Low' || payload.level === 'Normal',
      urgency: payload.level === 'Critical' ? 'critical' : 'normal',
    });

    notification.on('click', () => {
      // If there's a URL, open it in the browser.
      if (clickUrl) {
        shell.openExternal(clickUrl);
      }

      // Bring the app window to focus.
      const win = getMainWindow();
      if (win) {
        win.show();
        win.focus();
      }

      // Notify renderer about the click (for UI navigation).
      const win2 = getMainWindow();
      win2?.webContents.send('notification-click', {
        kind: payload.kind,
        item_id: 'item_id' in payload.kind ? payload.kind.item_id : undefined,
      });
    });

    notification.show();
  });
}
