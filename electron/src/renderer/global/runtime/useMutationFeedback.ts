import { useCallback, useMemo, useRef } from 'react';
import type {
  MutateOptions,
  MutationFunctionContext,
  UseMutationResult,
} from '@tanstack/react-query';

interface MutationFeedbackHandlers<TData, TError, TVariables, TContext> {
  onSuccess?: (
    data: TData,
    variables: TVariables,
    onMutateResult: TContext | undefined,
    context: MutationFunctionContext,
  ) => void;
  onError?: (
    error: TError,
    variables: TVariables,
    onMutateResult: TContext | undefined,
    context: MutationFunctionContext,
  ) => void;
  onSettled?: (
    data: TData | undefined,
    error: TError | null,
    variables: TVariables,
    onMutateResult: TContext | undefined,
    context: MutationFunctionContext,
  ) => void;
}

export function useMutationFeedback<TData, TError, TVariables, TContext>(
  mutation: UseMutationResult<TData, TError, TVariables, TContext>,
  handlers: MutationFeedbackHandlers<TData, TError, TVariables, TContext>,
): UseMutationResult<TData, TError, TVariables, TContext> {
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  const wrapOptions = useCallback(
    (
      options?: MutateOptions<TData, TError, TVariables, TContext>,
    ): MutateOptions<TData, TError, TVariables, TContext> | undefined => {
      const { onSuccess, onError, onSettled } = handlersRef.current;
      if (!onSuccess && !onError && !onSettled) {
        return options;
      }
      return {
        ...options,
        onSuccess: (data, variables, onMutateResult, context) => {
          onSuccess?.(data, variables, onMutateResult, context);
          options?.onSuccess?.(data, variables, onMutateResult, context);
        },
        onError: (error, variables, onMutateResult, context) => {
          onError?.(error, variables, onMutateResult, context);
          options?.onError?.(error, variables, onMutateResult, context);
        },
        onSettled: (data, error, variables, onMutateResult, context) => {
          onSettled?.(data, error, variables, onMutateResult, context);
          options?.onSettled?.(data, error, variables, onMutateResult, context);
        },
      };
    },
    [],
  );

  const baseMutate = mutation.mutate;
  const baseMutateAsync = mutation.mutateAsync;

  const mutate = useCallback<UseMutationResult<TData, TError, TVariables, TContext>['mutate']>(
    (variables, options) => {
      baseMutate(variables, wrapOptions(options));
    },
    [baseMutate, wrapOptions],
  );

  const mutateAsync = useCallback<
    UseMutationResult<TData, TError, TVariables, TContext>['mutateAsync']
  >(
    (variables, options) => baseMutateAsync(variables, wrapOptions(options)),
    [baseMutateAsync, wrapOptions],
  );

  return useMemo(() => ({ ...mutation, mutate, mutateAsync }), [mutation, mutate, mutateAsync]);
}
