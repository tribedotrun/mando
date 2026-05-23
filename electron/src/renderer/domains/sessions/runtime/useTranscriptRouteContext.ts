import { useSession } from '#renderer/domains/sessions/repo/queries';
import type { TaskProvider } from '#renderer/global/types';

export interface TranscriptRouteContext {
  provider: TaskProvider | undefined;
  cwd: string | undefined;
  isLoading: boolean;
}

export function useTranscriptRouteContext(
  sessionId: string,
  searchProvider: TaskProvider | undefined,
  searchCwd: string | undefined,
): TranscriptRouteContext {
  const needsLookup = !searchProvider || !searchCwd;
  const { data: sessionEntry, isLoading } = useSession(needsLookup ? sessionId : null);
  return {
    provider: searchProvider ?? sessionEntry?.provider,
    cwd: searchCwd ?? sessionEntry?.resume_cwd ?? sessionEntry?.cwd ?? undefined,
    isLoading: needsLookup && isLoading,
  };
}
