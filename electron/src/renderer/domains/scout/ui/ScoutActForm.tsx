import React from 'react';
import { Button } from '#renderer/global/ui/button';
import { Input } from '#renderer/global/ui/input';
import { Badge } from '#renderer/global/ui/badge';
import { Card, CardContent } from '#renderer/global/ui/card';
import { Combobox } from '#renderer/global/ui/combobox';

interface Props {
  projects: string[];
  actProject: string;
  setActProject: (v: string) => void;
  actPrompt: string;
  setActPrompt: (v: string) => void;
  acting: boolean;
  actResult: string | null;
  onAct: () => void;
}

export function ScoutActForm({
  projects,
  actProject,
  setActProject,
  actPrompt,
  setActPrompt,
  acting,
  actResult,
  onAct,
}: Props): React.ReactElement {
  return (
    <Card className="mb-5 py-3">
      <CardContent className="flex flex-col gap-3 px-4">
        <div className="flex items-center gap-2">
          {projects.length > 1 && (
            <Combobox
              value={actProject}
              onValueChange={setActProject}
              options={projects.map((p) => ({ value: p, label: p }))}
              placeholder="Select project..."
              searchPlaceholder="Search projects..."
              emptyText="No projects found."
              className="shrink-0 text-xs"
            />
          )}
          {projects.length === 1 && <Badge variant="secondary">{projects[0]}</Badge>}
        </div>
        <div className="flex items-center gap-2">
          <Input
            type="text"
            value={actPrompt}
            onChange={(e) => setActPrompt(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && actProject && !acting) onAct();
            }}
            placeholder="What should the task focus on? (optional)"
            className="h-8 min-w-0 flex-1 text-xs"
          />
          <Button size="sm" onClick={() => void onAct()} disabled={!actProject || acting}>
            {acting ? 'Creating...' : 'Create Task'}
          </Button>
        </div>
        {actResult && <div className="text-xs text-muted-foreground">{actResult}</div>}
      </CardContent>
    </Card>
  );
}
