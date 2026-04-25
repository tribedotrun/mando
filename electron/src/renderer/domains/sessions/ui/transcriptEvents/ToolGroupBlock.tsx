import React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
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
  const expanded = userOverride ?? false;

  return (
    <Collapsible open={expanded} onOpenChange={(v) => setToolExpanded(id, v)} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button className="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-label text-muted-foreground hover:bg-muted">
          <span className="font-medium text-foreground">✻</span>
          <span className="min-w-0 truncate opacity-80">{toolGroupSummary(tools)}</span>
          <span className="ml-auto">
            {expanded ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="pb-1 pl-2">
          {tools.map((t) => (
            <ToolCallBlock key={t.id} toolUse={t} result={results.get(t.id)} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
