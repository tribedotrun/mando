use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::panic::{catch_unwind, set_hook, take_hook, AssertUnwindSafe, PanicHookInfo};
use std::sync::Mutex;

use ts_rs::{Config, TypeVisitor, TS};

static PANIC_HOOK_LOCK: Mutex<()> = Mutex::new(());

macro_rules! register_roots {
    ($cfg:expr, $decls:expr, $seen:expr; $($ty:ty),+ $(,)?) => {
        $(register_decl::<$ty>($cfg, $decls, $seen);)+
    };
}

pub fn known_type_decls() -> BTreeMap<String, String> {
    let cfg = Config::new().with_large_int("number");
    let mut decls = BTreeMap::new();
    let mut seen = HashSet::new();

    register_roots!(
        &cfg,
        &mut decls,
        &mut seen;
        api_types::ActResponse,
        api_types::ActivityStatsResponse,
        api_types::AddProjectRequest,
        api_types::AdoptRequest,
        api_types::AdvisorActionResponse,
        api_types::AdvisorAskResponse,
        api_types::AdvisorRequest,
        api_types::AdvisorResponse,
        api_types::ArtifactEventData,
        api_types::ArtifactMedia,
        api_types::ArtifactMediaUpdateRequest,
        api_types::ArtifactRemoteUrlPatch,
        api_types::ArtifactType,
        api_types::ArtifactsPayload,
        api_types::ArtifactsResponse,
        api_types::AskEndResponse,
        api_types::AskHistoryEntry,
        api_types::AskHistoryResponse,
        api_types::AskReopenResponse,
        api_types::AskResponse,
        api_types::BoolOkResponse,
        api_types::BulkFailure,
        api_types::BoolTouchedResponse,
        api_types::CaptainConfig,
        api_types::ChannelStatus,
        api_types::ChannelsConfig,
        api_types::ChannelsResponse,
        api_types::ClarifierQuestion,
        api_types::ClarifyAnswer,
        api_types::ClarifyRequest,
        api_types::ClarifyResponse,
        api_types::ClientLogBatchRequest,
        api_types::ClientLogBatchResponse,
        api_types::ClientLogEntry,
        api_types::CredentialIdParams,
        api_types::ConfigPayload,
        api_types::ConfigPathsResponse,
        api_types::ConfigSaveResponse,
        api_types::ConfigSetupRequest,
        api_types::ConfigSetupResponse,
        api_types::ConfigStatusResponse,
        api_types::ConfigWriteResponse,
        api_types::CreateWorktreeRequest,
        api_types::CreateWorktreeResponse,
        api_types::CredentialInfo,
        api_types::CredentialListResponse,
        api_types::CredentialMutationResponse,
        api_types::CredentialProbeResponse,
        api_types::CredentialRateLimitStatus,
        api_types::CredentialTokenResponse,
        api_types::CredentialUsageSnapshot,
        api_types::CredentialWindowInfo,
        api_types::CredentialsEventData,
        api_types::CredentialsListResponse,
        api_types::CredentialsPayload,
        api_types::DailyMerge,
        api_types::DashboardConfig,
        api_types::DeleteTasksResponse,
        api_types::EditProjectRequest,
        api_types::EmptyRequest,
        api_types::EmptyResponse,
        api_types::ErrorResponse,
        api_types::EvidenceFileRequest,
        api_types::FeaturesConfig,
        api_types::FeedItem,
        api_types::FeedResponse,
        api_types::FirecrawlScrapeRequest,
        api_types::FirecrawlScrapeResponse,
        api_types::GatewayConfig,
        api_types::HealthResponse,
        api_types::ImageFilenameParams,
        api_types::InterestsConfig,
        api_types::ItemSessionsResponse,
        api_types::ItemStatus,
        api_types::MandoConfig,
        api_types::MergeRequest,
        api_types::MergeResponse,
        api_types::NotificationEventPayload,
        api_types::NotificationKind,
        api_types::NotificationPayload,
        api_types::NotifyLevel,
        api_types::NotifyRequest,
        api_types::NotifyResponse,
        api_types::NudgeRequest,
        api_types::NudgeResponse,
        api_types::ParseTodosRequest,
        api_types::ParseTodosResponse,
        api_types::PrSummaryResponse,
        api_types::ProbeCredentialResponse,
        api_types::ProcessResponse,
        api_types::ProjectAddResponse,
        api_types::ProjectConfig,
        api_types::ProjectDeleteResponse,
        api_types::ProjectNameParams,
        api_types::ProjectPatchResponse,
        api_types::ProjectsListResponse,
        api_types::ProjectUpsertResponse,
        api_types::RemoveWorktreeRequest,
        api_types::ResearchError,
        api_types::ResearchEventData,
        api_types::ResearchLink,
        api_types::ResearchPayload,
        api_types::ResearchStartResponse,
        api_types::ResyncPayload,
        api_types::ReviewTrigger,
        api_types::ScoutActRequest,
        api_types::ScoutAddRequest,
        api_types::ScoutAddResponse,
        api_types::ScoutArticleResponse,
        api_types::ScoutAskRequest,
        api_types::ScoutBulkDeleteRequest,
        api_types::ScoutBulkDeleteResponse,
        api_types::ScoutBulkCommandRequest,
        api_types::ScoutBulkUpdateResponse,
        api_types::ScoutConfig,
        api_types::ScoutDeleteResponse,
        api_types::ScoutEventData,
        api_types::ScoutItem,
        api_types::ScoutItemSession,
        api_types::ScoutPayload,
        api_types::ScoutProcessRequest,
        api_types::ScoutQuery,
        api_types::ScoutResearchRequest,
        api_types::ScoutResearchRun,
        api_types::ScoutResponse,
        api_types::ScoutLifecycleCommandRequest,
        api_types::ScoutItemLifecycleCommand,
        api_types::SessionCostResponse,
        api_types::SessionCostSummary,
        api_types::SessionEntry,
        api_types::SessionIdParams,
        api_types::SessionIds,
        api_types::SessionMessagesResponse,
        api_types::SessionMessagesQuery,
        api_types::SessionStatus,
        api_types::SessionStreamQuery,
        api_types::SessionSummary,
        api_types::SessionToolUsageResponse,
        api_types::SessionToolUsageSummary,
        api_types::SessionsEventData,
        api_types::SessionsListResponse,
        api_types::SessionsPayload,
        api_types::SessionsQuery,
        api_types::SessionsResponse,
        api_types::SetupTokenRequest,
        api_types::SetupTokenResponse,
        api_types::SnapshotErrorPayload,
        api_types::SnapshotPayload,
        api_types::SseDaemonInfo,
        api_types::SseEnvelope,
        api_types::SseResyncData,
        api_types::SseSnapshotData,
        api_types::SseSnapshotErrorData,
        api_types::StatusEventData,
        api_types::StatusPayload,
        api_types::StopWorkersResponse,
        api_types::SystemHealthResponse,
        api_types::TaskAddRequest,
        api_types::TaskArtifact,
        api_types::TaskAskRequest,
        api_types::TaskBulkUpdateRequest,
        api_types::TaskCreateResponse,
        api_types::TaskDeleteRequest,
        api_types::TaskEvidenceRequest,
        api_types::TaskEvidenceResponse,
        api_types::TaskEventData,
        api_types::TaskFeedbackRequest,
        api_types::TaskIdParams,
        api_types::TaskIdRequest,
        api_types::TaskItem,
        api_types::TaskListQuery,
        api_types::TaskListResponse,
        api_types::TaskPatchRequest,
        api_types::TaskSummaryRequest,
        api_types::TaskSummaryResponse,
        api_types::TasksPayload,
        api_types::TelegramConfig,
        api_types::TelegramHealth,
        api_types::TelegramOwnerRequest,
        api_types::TelegraphPublishResponse,
        api_types::TerminalAgent,
        api_types::TerminalCcSessionRequest,
        api_types::TerminalCreateRequest,
        api_types::TerminalExitPayload,
        api_types::TerminalIdParams,
        api_types::TerminalOutputPayload,
        api_types::TerminalSessionInfo,
        api_types::TerminalSize,
        api_types::TerminalState,
        api_types::TerminalStreamEnvelope,
        api_types::TerminalStreamQuery,
        api_types::TerminalWriteRequest,
        api_types::TickRequest,
        api_types::TickResult,
        api_types::TimelineEvent,
        api_types::TimelineResponse,
        api_types::TokenResponse,
        api_types::TranscriptMessage,
        api_types::TranscriptResponse,
        api_types::TranscriptToolCall,
        api_types::TranscriptUsageInfo,
        api_types::TriageItemResponse,
        api_types::TriageRequest,
        api_types::TriageResponse,
        api_types::UiConfig,
        api_types::UiDesiredState,
        api_types::UiHealthResponse,
        api_types::UiRegisterRequest,
        api_types::UserContextConfig,
        api_types::WorkbenchEventData,
        api_types::WorkbenchIdParams,
        api_types::WorkbenchItem,
        api_types::WorkbenchListQuery,
        api_types::WorkbenchPatchRequest,
        api_types::WorkbenchesPayload,
        api_types::WorkbenchesResponse,
        api_types::WorkerDetail,
        api_types::WorkerIdParams,
        api_types::WorkersResponse,
        api_types::WorktreeCleanupRequest,
        api_types::WorktreeCleanupResponse,
        api_types::WorktreeListItem,
        api_types::WorktreeListResponse,
        api_types::WorktreePruneError,
        api_types::WorktreePruneResponse,
        api_types::ArtifactIdParams,
        api_types::ArtifactMediaParams,
        api_types::ScoutItemIdParams,
        api_types::ScoutResearchIdParams,
        api_types::EvidenceCreatedResponse,
        api_types::SummaryCreatedResponse,
        api_types::ProjectSummary,
        api_types::TelegramReplyMarkup,
        api_types::InlineKeyboardButton,
        api_types::ActionKind,
        api_types::ClarifierQuestionPayload,
        api_types::TickAction,
        api_types::TickMode,
        api_types::TimelineEventPayload,
        api_types::ClientLogContext,
        api_types::CleanupWorktreesRequest,
        api_types::TaskBulkRequest,
        api_types::TaskBulkUpdates,
        api_types::EvidenceFileInput,
        api_types::EvidenceFilesRequest,
        api_types::WorkSummaryRequest,
        api_types::MessagesQuery,
        api_types::TranscriptLine,
        api_types::TranscriptInit,
        api_types::TranscriptUserEntry,
        api_types::TranscriptAssistantEntry,
        api_types::TranscriptToolUse,
        api_types::TranscriptToolResult,
        api_types::TranscriptResult,
        api_types::TranscriptSystem
    );

    decls
}

fn register_decl<T: TS + 'static>(
    cfg: &Config,
    decls: &mut BTreeMap<String, String>,
    seen: &mut HashSet<TypeId>,
) {
    register_decl_dyn::<T>(cfg, decls, seen);
}

fn register_decl_dyn<T: TS + 'static + ?Sized>(
    cfg: &Config,
    decls: &mut BTreeMap<String, String>,
    seen: &mut HashSet<TypeId>,
) {
    if !seen.insert(TypeId::of::<T>()) {
        return;
    }

    if let Some(name) = catch_quiet(|| T::ident(cfg)).filter(|name| should_emit_decl(name)) {
        if let Some(decl) = catch_quiet(|| T::decl_concrete(cfg)) {
            decls
                .entry(name)
                .or_insert_with(|| format!("export {}\n", decl));
        }
    }

    struct Visit<'a> {
        cfg: &'a Config,
        decls: &'a mut BTreeMap<String, String>,
        seen: &'a mut HashSet<TypeId>,
    }

    impl TypeVisitor for Visit<'_> {
        fn visit<U: TS + 'static + ?Sized>(&mut self) {
            register_decl_dyn::<U>(self.cfg, self.decls, self.seen);
        }
    }

    let mut visit = Visit { cfg, decls, seen };
    T::visit_dependencies(&mut visit);
    T::visit_generics(&mut visit);
}

fn should_emit_decl(name: &str) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    first.is_ascii_uppercase()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn catch_quiet<T>(f: impl FnOnce() -> T) -> Option<T> {
    let _hook_guard = PANIC_HOOK_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let hook = take_hook();
    set_hook(Box::new(|_: &PanicHookInfo<'_>| {}));
    let result = catch_unwind(AssertUnwindSafe(f)).ok();
    set_hook(hook);
    result
}
