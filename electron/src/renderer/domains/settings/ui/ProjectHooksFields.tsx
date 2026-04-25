import { Input } from '#renderer/global/ui/primitives/input';
import { Label } from '#renderer/global/ui/primitives/label';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/global/ui/primitives/collapsible';

interface ProjectHooksFieldsProps {
  preSpawn: string;
  setPreSpawn: (v: string) => void;
  workerTeardown: string;
  setWorkerTeardown: (v: string) => void;
  postMerge: string;
  setPostMerge: (v: string) => void;
}

export function ProjectHooksFields({
  preSpawn,
  setPreSpawn,
  workerTeardown,
  setWorkerTeardown,
  postMerge,
  setPostMerge,
}: ProjectHooksFieldsProps) {
  return (
    <Collapsible className="group">
      <CollapsibleTrigger className="cursor-pointer text-xs font-medium text-muted-foreground">
        Hooks (optional)
      </CollapsibleTrigger>
      <CollapsibleContent className="mt-3 space-y-3">
        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">pre_spawn</Label>
          <Input
            value={preSpawn}
            onChange={(e) => setPreSpawn(e.target.value)}
            placeholder="path/to/script.sh"
          />
        </div>
        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">worker_teardown</Label>
          <Input
            value={workerTeardown}
            onChange={(e) => setWorkerTeardown(e.target.value)}
            placeholder="path/to/script.sh"
          />
        </div>
        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">post_merge</Label>
          <Input
            value={postMerge}
            onChange={(e) => setPostMerge(e.target.value)}
            placeholder="path/to/script.sh"
          />
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}
