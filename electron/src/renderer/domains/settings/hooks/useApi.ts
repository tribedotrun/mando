/**
 * Re-exports API functions used by settings components.
 * Components import from here instead of '#renderer/api' directly
 * (enforced by the arch/no-api-in-components ESLint rule).
 */
export { apiGet, apiPatch, buildUrl } from '#renderer/api';
