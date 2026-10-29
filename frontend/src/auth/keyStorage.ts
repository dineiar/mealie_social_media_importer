import { parseApiKey } from "./apiKey";
import type { Credential, StoredKeyChange } from "./session";

const STORAGE_KEY = "mealie-importer.api-key";

type StorageArea = () => Storage;

export interface KeyStorage {
  read(): Credential | null;
  save(credential: Credential): void;
  clear(): void;
  subscribe(onChange: (change: StoredKeyChange) => void): () => void;
}

// The browser throws on any access to storage when site data is blocked.
function attempt<T>(action: () => T, fallback: T): T {
  try {
    return action();
  } catch {
    return fallback;
  }
}

export function createKeyStorage(
  persistent: StorageArea,
  tab: StorageArea,
): KeyStorage {
  const keyIn = (area: StorageArea) =>
    attempt(() => parseApiKey(area().getItem(STORAGE_KEY)), null);

  return {
    read() {
      const remembered = keyIn(persistent);
      if (remembered !== null) {
        return { key: remembered, remember: true };
      }
      const forTab = keyIn(tab);
      return forTab === null ? null : { key: forTab, remember: false };
    },

    save({ key, remember }) {
      const [keep, drop] = remember ? [persistent, tab] : [tab, persistent];
      attempt(() => drop().removeItem(STORAGE_KEY), undefined);
      attempt(() => keep().setItem(STORAGE_KEY, key), undefined);
    },

    clear() {
      attempt(() => persistent().removeItem(STORAGE_KEY), undefined);
      attempt(() => tab().removeItem(STORAGE_KEY), undefined);
    },

    subscribe(onChange) {
      function onStorage(event: StorageEvent) {
        const ofRememberedKey =
          event.storageArea === attempt(persistent, null) &&
          (event.key === STORAGE_KEY || event.key === null);
        if (!ofRememberedKey) {
          return;
        }
        const key = parseApiKey(event.newValue);
        onChange(
          key === null ? { kind: "forgotten" } : { kind: "remembered", key },
        );
      }
      window.addEventListener("storage", onStorage);
      return () => window.removeEventListener("storage", onStorage);
    },
  };
}

export const browserKeyStorage = createKeyStorage(
  () => window.localStorage,
  () => window.sessionStorage,
);
