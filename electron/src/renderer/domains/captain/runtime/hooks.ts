// Query hooks
export {
  useTaskList,
  useTaskListWithArchived,
  useTaskAskHistory,
  useTaskFeed,
  useTaskArtifacts,
  useTerminalList,
  useWorkbenchList,
  useActivityStats,
  useWorkers,
  useTaskTimelineData,
  useTaskPrSummary,
  type TerminalSessionInfo,
} from '#renderer/domains/captain/repo/queries';

// Mutation hooks
export {
  useTaskCreate,
  useTaskAccept,
  useTaskCancel,
  useTaskRetry,
  useResumeRateLimited,
  useTaskHandoff,
  useTaskStop,
  useTaskReopen,
  useTaskAskReopen,
  useTaskRework,
  useTaskMerge,
  useTaskAsk,
  useTaskAdvisor,
  useTaskNudge,
  useTaskDelete,
  useTaskClarify,
  useTaskBulkCreate,
  useStartImplementation,
} from '#renderer/domains/captain/runtime/useFeedbackTaskMutations';

// Extra mutation hooks (split for file-length compliance)
export { useEndAskSession } from '#renderer/domains/captain/repo/mutations-extra';
export { useAddProject } from '#renderer/domains/captain/runtime/useFeedbackTaskMutations';

// Terminal mutation hooks
export {
  useTerminalCreate,
  useTerminalDelete,
  useWorkbenchPin,
  useWorkbenchRename,
  useWorkbenchArchive,
  useWorkbenchUnarchive,
} from '#renderer/domains/captain/terminal/runtime/useFeedbackTerminalMutations';

// Activity strip data hook (runtime, not repo)
export { useActivityStripData } from '#renderer/domains/captain/runtime/useActivityStripData';

// Query key re-export for imperative cache access in terminal UI
export { queryKeys } from '#renderer/global/repo/queryKeys';
