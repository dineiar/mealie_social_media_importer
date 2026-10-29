// A browser throws on any access to storage when it blocks site data,
// and may lack or refuse BroadcastChannel.
export function attempt<T>(action: () => T, fallback: T): T {
  try {
    return action();
  } catch {
    return fallback;
  }
}
