/**
 * Re-exports API functions used by global components.
 * Components import from here instead of '#renderer/api' directly
 * (enforced by the arch/no-api-in-components ESLint rule).
 */
export { buildUrl } from '#renderer/global/providers/http';

import { buildUrl as _buildUrl, staticRoutePath } from '#renderer/global/providers/http';

/** Builds a full URL for a project logo image. */
export function projectLogoUrl(logoFile: string): string {
  return _buildUrl(staticRoutePath('getImagesByFilename', { params: { filename: logoFile } }));
}
