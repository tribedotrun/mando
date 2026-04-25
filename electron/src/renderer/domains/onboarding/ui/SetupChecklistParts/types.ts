export type StepId = 'project' | 'claude-code' | 'telegram';

export interface StepDef {
  id: StepId;
  title: string;
  completed: boolean;
  expandable: boolean;
}
