import { attempt } from "./attempt";

const CHANNEL_NAME = "mealie-importer.log-off";

const open = () => attempt(() => new BroadcastChannel(CHANNEL_NAME), null);

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
