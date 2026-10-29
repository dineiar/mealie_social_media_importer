const CHANNEL_NAME = "mealie-importer.log-off";

// A browser may lack BroadcastChannel or throw when it blocks site data.
function open(): BroadcastChannel | null {
  try {
    return new BroadcastChannel(CHANNEL_NAME);
  } catch {
    return null;
  }
}

export function announceLogOff(): void {
  const channel = open();
  channel?.postMessage("logged_off");
  channel?.close();
}

export function listenForLogOff(onLogOff: () => void): () => void {
  const channel = open();
  channel?.addEventListener("message", onLogOff);
  return () => channel?.close();
}
