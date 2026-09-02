import { staticRoutePath } from '#renderer/global/providers/http';
import { buildUrl } from '#renderer/global/runtime/useApi';

/** Build the protected URL for one image embedded in a session tool result. */
export function sessionToolResultImageUrl(
  sessionId: string,
  toolUseId: string,
  index: number,
): string {
  return buildUrl(
    staticRoutePath('getSessionsByIdImagesByToolByIndex', {
      params: { id: sessionId, tool: toolUseId, index },
    }),
  );
}
