import React from 'react';
import { Button } from '#renderer/global/ui/primitives/button';
import { Input } from '#renderer/global/ui/primitives/input';
import { Badge } from '#renderer/global/ui/primitives/badge';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Combobox } from '#renderer/global/ui/primitives/combobox';
import { useScoutActForm } from '#renderer/domains/scout/runtime/useScoutActForm';

interface Props {
  itemId: number;
  open: boolean;
}

export function ScoutActForm({ itemId, open }: Props): React.ReactElement {
  const act = useScoutActForm(itemId, open);

  return (
    <Card className="mb-5 py-3">
      <CardContent className="flex flex-col gap-3 px-4">
        <div className="flex items-center gap-2">
          {act.projects.length > 1 && (
            <Combobox
              value={act.project}
              onValueChange={act.setProject}
              options={act.projects.map((p) => ({ value: p, label: p }))}
              placeholder="Select project..."
              searchPlaceholder="Search projects..."
              emptyText="No projects found."
              className="shrink-0 text-xs"
            />
          )}
          {act.projects.length === 1 && <Badge variant="secondary">{act.projects[0]}</Badge>}
        </div>
        <div className="flex items-center gap-2">
          <Input
            type="text"
            value={act.prompt}
            onChange={(e) => act.setPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && act.project && !act.pending) act.handleAct();
            }}
            placeholder="What should the task focus on? (optional)"
            className="h-8 min-w-0 flex-1 text-xs"
          />
          <Button
            size="sm"
            onClick={() => void act.handleAct()}
            disabled={!act.project || act.pending}
          >
            {act.pending ? 'Creating...' : 'Create Task'}
          </Button>
        </div>
        {act.result && <div className="text-xs text-muted-foreground">{act.result}</div>}
      </CardContent>
    </Card>
  );
}
