export interface ClarifierFailedPayload {
  event_type: 'clarifier_failed';
  // PR #889: sentinel "" == no CC session established (pre-prompt failure).
  session_id: string;
  // PR #889: sentinel 0 == non-HTTP error (transport/internal).
  api_error_status: number;
  message: string;
}
