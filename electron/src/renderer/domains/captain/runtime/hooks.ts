// Query hooks
export {
  useTaskList,
  useTaskListWithArchived,
  useTaskAskHistory,
  useTaskFeed,
  useTaskArtifacts,
  useWorkbenchList,
  useActivityStats,
  useWorkers,
  useTaskTimelineData,
  useTaskPrSummary,
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
  useTaskSetIsBugFix,
} from '#renderer/domains/captain/runtime/useFeedbackTaskMutations';

// Extra mutation hooks (split for file-length compliance)
export { useEndAskSession } from '#renderer/domains/captain/repo/mutations-extra';
export { useAddProject } from '#renderer/domains/captain/runtime/useFeedbackTaskMutations';

// Workbench mutation hooks
export {
  useWorkbenchPin,
  useWorkbenchRename,
  useWorkbenchArchive,
  useWorkbenchUnarchive,
} from '#renderer/domains/captain/runtime/useFeedbackWorkbenchMutations';

// Activity strip data hook (runtime, not repo)
export { useActivityStripData } from '#renderer/domains/captain/runtime/useActivityStripData';
