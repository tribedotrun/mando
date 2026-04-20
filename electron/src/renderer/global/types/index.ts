import type {
  ItemStatus,
  SessionsEventData,
  SseEnvelope,
  StatusEventData,
  TaskItem,
} from '#shared/daemon-contract';

export type {
  ActResponse,
  ActivityStatsResponse,
  AdvisorActionResponse,
  AdvisorAskResponse,
  AdvisorResponse,
  ArtifactMedia,
  ArtifactsResponse,
  AskHistoryEntry,
  AskHistoryResponse,
  AskResponse,
  ClarifyResponse,
  ClarifierQuestion,
  ClarifierQuestionPayload,
  CreateWorktreeResponse,
  CredentialInfo,
  DailyMerge,
  FeedItem,
  FeedResponse,
  ItemSessionsResponse,
  ItemStatus,
  MergeResponse,
  NudgeResponse,
  PrSummaryResponse,
  ReviewTrigger,
  ScoutArticleResponse,
  ScoutEventData,
  ScoutItem,
  ScoutItemSession,
  ScoutResearchRun,
  SessionsEventData,
  ScoutResponse,
  SessionEntry,
  SessionIds,
  SessionSummary,
  SessionsResponse,
  StatusEventData,
  TaskEventData,
  TaskArtifact,
  TaskCreateResponse,
  SseSnapshotData,
  TaskItem,
  TaskListResponse,
  TelegramHealth,
  TerminalSessionInfo,
  TickResult,
  TimelineEvent,
  TimelineResponse,
  TranscriptResponse,
  ResearchEventData,
  ArtifactEventData,
  WorkbenchEventData,
  WorkbenchItem,
  WorkerDetail,
  WorkersResponse,
} from '#shared/daemon-contract';
export type {
  CaptainConfig,
  FeaturesConfig,
  MandoConfig,
  ProjectConfig,
  ScoutConfig,
  TelegramConfig,
  UiConfig,
} from '#renderer/global/types/config';

export const FINALIZED_STATUSES: ItemStatus[] = ['merged', 'completed-no-pr', 'canceled'];

export const ACTION_NEEDED_STATUSES: ItemStatus[] = [
  'awaiting-review',
  'escalated',
  'needs-clarification',
  'plan-ready',
];

export const IN_PROGRESS_STATUSES: ItemStatus[] = [
  'clarifying',
  'in-progress',
  'captain-reviewing',
  'captain-merging',
];

export const WORKING_STATUSES: ItemStatus[] = [
  'in-progress',
  'clarifying',
  'rework',
  'handed-off',
  'captain-reviewing',
  'captain-merging',
];

export const ALL_STATUSES: ItemStatus[] = [
  'new',
  'clarifying',
  'needs-clarification',
  'queued',
  'in-progress',
  'captain-reviewing',
  'captain-merging',
  'awaiting-review',
  'rework',
  'handed-off',
  'escalated',
  'errored',
  'merged',
  'completed-no-pr',
  'plan-ready',
  'canceled',
];

export type SSEConnectionStatus = 'connected' | 'connecting' | 'disconnected';

export type SSEEvent = SseEnvelope;

export type SseAction = 'created' | 'updated' | 'deleted';

export interface SseEntityPayload<T> {
  action?: string | null;
  item?: T | null;
  id?: number | string | null;
}

export type SseStatusPayload = StatusEventData;
export type SseSessionsPayload = SessionsEventData;

export type WorkbenchStatusFilter = 'active' | 'archived' | 'all';

export interface PinnedWorkbench {
  id: number;
  worktree: string;
  title: string;
  createdAt: string;
  lastActivityAt?: string;
  pinnedAt?: string | null;
  archivedAt?: string | null;
}

export interface PinnedEntry {
  wb: PinnedWorkbench;
  task?: TaskItem;
  project: string;
}

export const WORKBENCH_FILTER_OPTIONS: WorkbenchStatusFilter[] = ['active', 'archived', 'all'];

export type { NotificationKind, NotificationPayload, NotifyLevel } from '#shared/notifications';

declare global {
  interface Window {
    mandoAPI: import('#preload/index').MandoAPI;
    __devInspectorCopy?: () => void;
    __buildComponentMap?: () => unknown[];
  }
}
