import React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '#renderer/global/ui/primitives/collapsible';
import {
  selectToolOpenState,
  useTranscriptUi,
} from '#renderer/domains/sessions/runtime/useTranscriptUi';

interface ToolFrameProps {
  id: string;
  name: string;
  summary?: React.ReactNode;
  isError?: boolean;
  resultBadge?: React.ReactNode;
  children?: React.ReactNode;
  defaultOpen?: boolean;
}

export function ToolFrame({
  id,
  name,
  summary,
  isError,
  resultBadge,
  children,
  defaultOpen = false,
}: ToolFrameProps): React.ReactElement {
  const userOverride = useTranscriptUi(selectToolOpenState(id));
  const setToolExpanded = useTranscriptUi((s) => s.setToolExpanded);
  const open = userOverride ?? defaultOpen;
  const hasBody = Boolean(children);

  return (
    <Collapsible open={open} onOpenChange={(v) => setToolExpanded(id, v)} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button
          className={`flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-label text-muted-foreground${hasBody ? ' hover:bg-muted' : ''}`}
          style={{ cursor: hasBody ? 'pointer' : 'default' }}
          onClick={(e) => {
            if (!hasBody) e.preventDefault();
          }}
        >
          <span className={`font-medium ${isError ? 'text-destructive' : 'text-foreground'}`}>
            {name}
          </span>
          {summary && (
            <span className="min-w-0 truncate text-label normal-case opacity-60">{summary}</span>
          )}
          <span className="ml-auto flex items-center gap-1">
            {resultBadge}
            {hasBody && (open ? <ChevronDown size={11} /> : <ChevronRight size={11} />)}
          </span>
        </button>
      </CollapsibleTrigger>
      {hasBody && (
        <CollapsibleContent>
          <div className="pb-2 pl-2">{children}</div>
        </CollapsibleContent>
      )}
    </Collapsible>
  );
}
