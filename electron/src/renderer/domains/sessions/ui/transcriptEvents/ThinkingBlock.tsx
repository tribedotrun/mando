import React from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '#renderer/global/ui/primitives/collapsible';
import {
  selectIsThinkingOpen,
  useTranscriptUi,
} from '#renderer/domains/sessions/runtime/useTranscriptUi';

interface ThinkingBlockProps {
  id: string;
  text: string;
}

export function ThinkingBlock({ id, text }: ThinkingBlockProps): React.ReactElement {
  const open = useTranscriptUi(selectIsThinkingOpen(id));
  const toggle = useTranscriptUi((s) => s.toggleThinking);
  const preview = text.trim().slice(0, 120);
  return (
    <Collapsible open={open} onOpenChange={() => toggle(id)} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button className="flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-label italic text-muted-foreground hover:bg-muted">
          <span className="font-medium">thinking</span>
          {!open && <span className="min-w-0 truncate opacity-70">{preview}…</span>}
          <span className="ml-auto">
            {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          </span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <pre className="mt-1 whitespace-pre-wrap rounded bg-muted/40 px-3 py-2 text-label italic text-muted-foreground">
          {text}
        </pre>
      </CollapsibleContent>
    </Collapsible>
  );
}
