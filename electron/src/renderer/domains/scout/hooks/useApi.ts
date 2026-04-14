/**
 * Re-exports API functions used by scout components.
 * Components import from here instead of '#renderer/api' directly
 * (enforced by the arch/no-api-in-components ESLint rule).
 */
export {
  actOnScoutItem,
  askScout,
  bulkDeleteScout,
  bulkUpdateScout,
  fetchScoutArticle,
  fetchScoutItem,
  researchScout,
  updateScoutStatus,
} from '#renderer/api';
