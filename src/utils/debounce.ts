export type Debounced<TArgs extends unknown[]> = ((...args: TArgs) => void) & {
  cancel: () => void;
};

/**
 * Debounce a function so it only fires after `ms` of inactivity.
 */
export function debounce<TArgs extends unknown[]>(
  fn: (...args: TArgs) => void,
  ms: number
): Debounced<TArgs> {
  let timeout: ReturnType<typeof setTimeout> | undefined;

  const cancel = () => {
    if (timeout !== undefined) {
      clearTimeout(timeout);
      timeout = undefined;
    }
  };

  const debounced = ((...args: TArgs) => {
    cancel();
    timeout = setTimeout(() => {
      timeout = undefined;
      fn(...args);
    }, ms);
  }) as Debounced<TArgs>;

  debounced.cancel = cancel;
  return debounced;
}
