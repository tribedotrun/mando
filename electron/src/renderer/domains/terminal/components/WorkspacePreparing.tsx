import { Spinner } from '#renderer/global/components/Spinner';

interface WorkspacePreparingProps {
  project: string;
  onCancel: () => void;
}

export function WorkspacePreparing({ project, onCancel }: WorkspacePreparingProps) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3">
      <Spinner size={20} />
      <span className="text-body text-text-2">Creating workspace for {project}</span>
      <button onClick={onCancel} className="mt-2 text-caption text-text-3 hover:text-text-2">
        Cancel
      </button>
    </div>
  );
}
