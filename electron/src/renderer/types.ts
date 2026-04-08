// Types matching the Rust mando-types crate and REST API contract

export type ItemStatus =
  | 'new'
  | 'clarifying'
  | 'needs-clarification'
  | 'queued'
  | 'in-progress'
  | 'captain-reviewing'
  | 'captain-merging'
  | 'awaiting-review'
  | 'rework'
  | 'handed-off'
  | 'escalated'
  | 'errored'
  | 'merged'
  | 'completed-no-pr'
  | 'canceled';

export const FINALIZED_STATUSES: ItemStatus[] = ['merged', 'completed-no-pr', 'canceled'];

/** Statuses that require human action — used for the "Action needed" filter tab */
export const ACTION_NEEDED_STATUSES: ItemStatus[] = [
  'awaiting-review',
  'escalated',
  'needs-clarification',
];

/** Statuses where work is actively happening — used for the "In progress" filter tab */
export const IN_PROGRESS_STATUSES: ItemStatus[] = [
  'clarifying',
  'in-progress',
  'captain-reviewing',
  'captain-merging',
];

export interface TaskItem {
  id: number;
  title: string;
  status: ItemStatus;
  project?: string;
  github_repo?: string;
  branch?: string;
  pr_number?: number;
  project_id?: number;
  worker?: string;
  session_ids?: {
    worker?: string;
    review?: string;
    clarifier?: string;
    merge?: string;
    ask?: string;
  };
  intervention_count: number;
  captain_review_trigger?: string;
  escalation_report?: string;
  context?: string;
  original_prompt?: string;
  workbench_id?: number;
  worktree?: string;
  plan?: string;
  no_pr?: boolean;
  resource?: string;
  images?: string;
  created_at?: string;
  last_activity_at?: string;
  worker_started_at?: string;
  worker_seq: number;
  reopen_seq: number;
  reopen_source?: string;
  review_fail_count: number;
  clarifier_fail_count: number;
  spawn_fail_count: number;
  merge_fail_count: number;
  source?: string;
  archived_at?: string;
}

export interface TaskListResponse {
  items: TaskItem[];
  count: number;
}

export interface WorkerDetail {
  id: number;
  title: string;
  status?: ItemStatus;
  project: string;
  github_repo?: string;
  branch?: string;
  cc_session_id?: string | null;
  worker?: string;
  worktree?: string;
  pr_number?: number | null;
  started_at?: string;
  last_activity_at?: string;
  intervention_count?: number;
  nudge_count?: number;
  nudge_budget?: number;
  last_action?: string;
  pid?: number;
  is_stale?: boolean;
}

export interface WorkersResponse {
  workers: WorkerDetail[];
  rate_limit_remaining_secs?: number;
}

export interface ScoutItem {
  id: number;
  url: string;
  title?: string;
  status: string;
  item_type?: string;
  summary?: string;
  has_summary?: boolean;
  relevance?: number;
  quality?: number;
  date_added?: string;
  source_name?: string;
  date_published?: string;
  telegraphUrl?: string;
}

export interface ScoutResponse {
  items: ScoutItem[];
  count: number;
  total: number;
  page: number;
  pages: number;
  per_page: number;
  filter: string | null;
  status_counts?: Record<string, number>;
}

type SessionStatus = 'running' | 'stopped' | 'failed';

export interface SessionEntry {
  session_id: string;
  created_at: string;
  cwd: string;
  model: string;
  caller: string;
  resumed: boolean | number;
  cost_usd?: number;
  duration_ms?: number;
  turn_count?: number;
  scout_item_id?: number | null;
  task_id: string | null;
  worker_name: string | null;
  status: SessionStatus;
  task_title?: string;
  scout_item_title?: string;
  github_repo?: string;
  pr_number?: number;
  worktree?: string;
  branch?: string;
  resume_cwd?: string;
  category?: string;
}

export interface TranscriptResponse {
  session_id: string;
  markdown: string;
}

export interface SessionsResponse {
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
  categories: Record<string, number>;
  total_cost_usd?: number;
  sessions: SessionEntry[];
}

export interface SessionSummary {
  session_id: string;
  status: SessionStatus;
  caller: string;
  started_at: string;
  duration_ms?: number;
  cost_usd?: number;
  model?: string;
  resumed: boolean;
  cwd?: string;
  worker_name?: string;
}

export interface ItemSessionsResponse {
  sessions: SessionSummary[];
  count: number;
}

export interface ClarifierQuestion {
  question: string;
  answer?: string | null;
  self_answered: boolean;
  category?: 'code' | 'intent';
}

export interface TimelineEvent {
  event_type: string;
  timestamp: string;
  actor: string;
  summary: string;
  data: Record<string, unknown>;
}

export interface TimelineResponse {
  id: string;
  events: TimelineEvent[];
  count: number;
}

export interface TickResult {
  mode: string;
  tick_id?: string;
  actions?: unknown[];
  tasks?: Record<string, number>;
  alerts?: string[];
  rate_limited?: boolean;
}

export interface PrSummaryResponse {
  pr_number: number;
  summary: string | null;
}

export interface AskResponse {
  answer: string;
  session_id?: string;
  suggested_followups?: string[];
}

export interface AskHistoryEntry {
  role: 'human' | 'assistant';
  content: string;
  timestamp: string;
}

export interface AskHistoryResponse {
  history: AskHistoryEntry[];
}

export interface ScoutArticleResponse {
  article: string;
  title?: string;
  telegraphUrl?: string;
}

export interface ActResponse {
  ok?: boolean;
  task_id?: string;
  title?: string;
  skipped?: boolean;
  reason?: string;
}

export type SSEConnectionStatus = 'connected' | 'connecting' | 'disconnected';

export interface SSEEvent {
  event: string;
  ts: number;
  data?: unknown;
}

// ── Desktop notification types (re-exported from shared module) ──
export type { NotifyLevel, NotificationPayload } from '#shared/notifications';

export interface ScoutItemSession {
  session_id: string;
  caller: string;
  status: string;
  created_at: string;
  model?: string;
  duration_ms?: number | null;
  cost_usd?: number | null;
}

// Window augmentation for preload API
declare global {
  interface Window {
    mandoAPI: import('../preload/index').MandoAPI;
  }
}
