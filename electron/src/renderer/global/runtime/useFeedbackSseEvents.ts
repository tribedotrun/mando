import { toast } from '#renderer/global/runtime/useFeedback';
import { parseNotification } from '#renderer/global/service/notificationHelpers';
import type { SSEEvent } from '#renderer/global/types';
import log from '#renderer/global/service/logger';

export function handleFeedbackSseEvent(event: SSEEvent): void {
  switch (event.event) {
    case 'notification': {
      const payload = parseNotification(event);
      if (payload) {
        if (payload.kind?.type === 'RateLimited') {
          const notify = payload.kind.status === 'rejected' ? toast.error : toast.info;
          notify(payload.message);
        }
      } else if (event.data.data) {
        log.warn('[sse] unexpected notification shape:', event.data);
      }
      break;
    }

    case 'research': {
      const payload = event.data.data;
      if (!payload) break;
      if (payload.action === 'completed') {
        const added = payload.added_count ?? 0;
        toast.success(`Research complete: ${added} link(s) added`);
      } else if (payload.action === 'failed') {
        const error = payload.error ?? 'Unknown error';
        toast.error(`Research failed: ${error}`);
      }
      break;
    }

    default:
      break;
  }
}
