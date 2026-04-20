import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/global/ui/collapsible';
import { type PromptGroup, countToolCalls } from '#renderer/domains/sessions/service/transcript';
import {
  HumanSection,
  TurnContent,
  SectionRow,
} from '#renderer/domains/sessions/ui/TranscriptBlocksParts';

export function PromptGroupRow({ group }: { group: PromptGroup }): React.ReactElement {
  const lastTurn = group.turns.length > 0 ? group.turns[group.turns.length - 1] : null;
  const intermediateTurns = group.turns.slice(0, -1);
  const [showIntermediate, setShowIntermediate] = useState(false);

  const intermediateToolCount = countToolCalls(intermediateTurns);

  return (
    <div className="py-3">
      {group.prompt &&
        (group.prompt.kind === 'session-end' ? (
          <div className="pt-4 pb-1 text-label italic text-muted-foreground">
            {group.prompt.heading.replace(/^\*|\*$/g, '')}
          </div>
        ) : (
          <HumanSection section={group.prompt} />
        ))}

      {intermediateTurns.length > 0 && (
        <Collapsible open={showIntermediate} onOpenChange={setShowIntermediate}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="xs" className="my-2 gap-1.5 text-muted-foreground">
              {showIntermediate ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span>
                {intermediateTurns.length} turn{intermediateTurns.length !== 1 ? 's' : ''}
                {intermediateToolCount > 0 &&
                  `, ${intermediateToolCount} tool call${intermediateToolCount !== 1 ? 's' : ''}`}
              </span>
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            {intermediateTurns.map((turn, ti) => (
              <SectionRow key={ti} section={turn} />
            ))}
          </CollapsibleContent>
        </Collapsible>
      )}

      {lastTurn && <TurnContent section={lastTurn} />}
    </div>
  );
}
