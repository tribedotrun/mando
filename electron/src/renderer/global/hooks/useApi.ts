/**
 * Re-exports API functions used by global components.
 * Components import from here instead of '#renderer/api' directly
 * (enforced by the arch/no-api-in-components ESLint rule).
 */
export { buildUrl } from '#renderer/api';
