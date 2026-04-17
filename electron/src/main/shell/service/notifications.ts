import type { NotificationKind } from '#shared/notifications';

export function titleForKind(kind: NotificationKind): string {
  switch (kind.type) {
    case 'Escalated':
      return 'Escalated';
    case 'NeedsClarification':
      return 'Clarification Needed';
    case 'RateLimited':
      return 'Rate Limited';
    case 'ScoutProcessed':
      return 'Scout Processed';
    case 'ScoutProcessFailed':
      return 'Scout Failed';
    case 'AdvisorAnswered':
      return 'Advisor Answered';
    case 'Generic':
      return 'Mando';
  }
}

export function stripHtml(html: string): string {
  return html.replace(/<[^>]*>/g, '');
}
