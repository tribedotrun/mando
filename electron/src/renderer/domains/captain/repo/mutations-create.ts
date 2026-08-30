import { useMutation } from '@tanstack/react-query';
import { addTask, type AddTaskInput } from '#renderer/domains/captain/repo/api';
import { toReactQuery } from '#result';

export function useTaskCreate() {
  return useMutation({
    mutationFn: (input: AddTaskInput) => toReactQuery(addTask(input)),
  });
}
