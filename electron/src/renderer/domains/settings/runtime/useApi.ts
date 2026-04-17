/**
 * Re-exports API functions used by settings components.
 * Components import from here instead of '#renderer/api' directly
 * (enforced by the arch/no-api-in-components ESLint rule).
 */
export { apiGet, apiPost, apiDel, apiPatch, buildUrl } from '#renderer/global/providers/http';
export { projectLogoUrl } from '#renderer/global/runtime/useApi';
