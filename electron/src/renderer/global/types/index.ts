import type {
  ItemStatus,
  SessionsEventData,
  SseEnvelope,
  StatusEventData,
  TaskItem,
  WorkbenchStatusFilter,
} from '#shared/daemon-contract';
import { itemStatusSchema, workbenchStatusFilterSchema } from '#shared/daemon-contract/schemas';

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
  BulkResultStatus,
  ClarifyOutcome,
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
  ScoutItemLifecycleCommand,
  ScoutItemStatus,
  ScoutItemStatusFilter,
  ScoutResearchRun,
  ScoutResearchRunStatus,
  SessionsEventData,
  SessionStatus,
  ScoutResponse,
  SessionEntry,
  SessionCategory,
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
  TerminalState,
  TerminalSessionInfo,
  TickResult,
  TimelineEvent,
  TimelineResponse,
  TranscriptEvent,
  TranscriptEventEnvelope,
  TranscriptEventsResponse,
  AssistantContentBlock,
  UserContentBlock,
  UserToolResultBlock,
  ToolName,
  ToolInput,
  ResultEvent,
  ResultOutcome,
  ResultSummary,
  SystemInitEvent,
  SystemCompactBoundaryEvent,
  SystemStatusEvent,
  SystemApiRetryEvent,
  SystemLocalCommandOutputEvent,
  SystemHookEvent,
  SystemRateLimitEvent,
  HookPhase,
  AssistantEvent,
  AssistantToolUseBlock,
  UserEvent,
  ToolProgressEvent,
  UnknownEvent,
  EventMeta,
  CcTodoItemStatus,
  CcTodoItem,
  McpToolName,
  OtherToolName,
  BashInput,
  ReadInput,
  EditInput,
  WriteInput,
  GrepInput,
  GlobInput,
  TodoWriteInput,
  WebFetchInput,
  WebSearchInput,
  TaskInput,
  NotebookEditInput,
  SkillInput,
  StructuredOutputInput,
  OpaqueInput,
  ModelUsageBreakdown,
  TranscriptUsageInfo,
  ResearchEventData,
  ArtifactEventData,
  WorkbenchEventData,
  WorkbenchItem,
  WorkbenchStatusFilter,
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

export const FINALIZED_STATUSES: readonly ItemStatus[] = ['merged', 'completed-no-pr', 'canceled'];

export const ACTION_NEEDED_STATUSES: readonly ItemStatus[] = [
  'awaiting-review',
  'escalated',
  'needs-clarification',
  'plan-ready',
];

export const IN_PROGRESS_STATUSES: readonly ItemStatus[] = [
  'clarifying',
  'in-progress',
  'captain-reviewing',
  'captain-merging',
];

export const WORKING_STATUSES: readonly ItemStatus[] = [
  'in-progress',
  'clarifying',
  'rework',
  'handed-off',
  'captain-reviewing',
  'captain-merging',
];

export const ALL_STATUSES: readonly ItemStatus[] = itemStatusSchema.options;

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

export const WORKBENCH_FILTER_OPTIONS: readonly WorkbenchStatusFilter[] =
  workbenchStatusFilterSchema.options;

export type { NotificationKind, NotificationPayload, NotifyLevel } from '#shared/notifications';

declare global {
  interface Window {
    mandoAPI: import('#preload/index').MandoAPI;
    __devInspectorCopy?: () => void;
    __buildComponentMap?: () => unknown[];
  }
}
