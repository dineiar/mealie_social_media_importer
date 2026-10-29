// A browser throws on any access to storage when it blocks site data.
export function attempt<T>(action: () => T, fallback: T): T {
  try {
    return action();
  } catch {
    return fallback;
  }
}
