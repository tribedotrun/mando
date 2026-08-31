/** Re-exports URL builders through the renderer runtime boundary. */
export { buildUrl } from '#renderer/global/providers/http';

import { buildUrl as _buildUrl, staticRoutePath } from '#renderer/global/providers/http';

function storedImageUrl(filename: string): string {
  return _buildUrl(staticRoutePath('getImagesByFilename', { params: { filename } }));
}

/** Builds a full URL for a project logo image. */
export const projectLogoUrl = storedImageUrl;

/** Builds a full URL for a task-attached image. */
export const taskImageUrl = storedImageUrl;
