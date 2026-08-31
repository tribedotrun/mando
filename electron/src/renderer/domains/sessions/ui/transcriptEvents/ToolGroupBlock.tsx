import React from 'react';
import { ChevronDown, ChevronRight, ListTree } from 'lucide-react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '#renderer/global/ui/primitives/collapsible';
import type { AssistantToolUseBlock, UserToolResultBlock } from '#renderer/global/types';
import { toolGroupSummary } from '#renderer/domains/sessions/service/transcriptRenderHelpers';
import { ToolCallBlock } from '#renderer/domains/sessions/ui/transcriptEvents/ToolCallBlock';
import {
  selectToolOpenState,
  useTranscriptUi,
} from '#renderer/domains/sessions/runtime/useTranscriptUi';

interface ToolGroupBlockProps {
  id: string;
  tools: AssistantToolUseBlock[];
  results: Map<string, UserToolResultBlock>;
}

export function ToolGroupBlock({ id, tools, results }: ToolGroupBlockProps): React.ReactElement {
  const userOverride = useTranscriptUi(selectToolOpenState(id));
  const setToolExpanded = useTranscriptUi((s) => s.setToolExpanded);
  const failedCount = tools.filter((tool) => results.get(tool.id)?.isError === true).length;
  const expanded = userOverride ?? failedCount > 0;
  const summary = toolGroupSummary(tools);

  return (
    <Collapsible open={expanded} onOpenChange={(v) => setToolExpanded(id, v)} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button
          data-testid="tool-activity-group"
          className="flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left text-label text-muted-foreground hover:bg-muted/70"
        >
          <span className="shrink-0 text-text-3">
            <ListTree size={14} />
          </span>
          <span className="min-w-0 truncate font-medium text-foreground">{summary}</span>
          <span className="ml-auto flex items-center gap-2">
            {failedCount > 0 && <span className="text-destructive">{failedCount} failed</span>}
            {expanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="ml-4 border-l border-border/60 pb-1 pl-2">
          {tools.map((t) => (
            <ToolCallBlock key={t.id} toolUse={t} result={results.get(t.id)} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
