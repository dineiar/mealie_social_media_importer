import { type ApiKey, parseApiKey } from "./apiKey";
import type { Credential } from "./session";

export const STORAGE_KEY = "mealie-importer.api-key";

type StorageArea = () => Storage;

export interface KeyStorage {
  read(): Credential | null;
  save(credential: Credential): void;
  discard(credential: Credential): void;
  clear(): void;
  subscribe(onChange: (key: ApiKey | null) => void): () => void;
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
      const forTab = keyIn(tab);
      if (forTab !== null) {
        return { key: forTab, remember: false };
      }
      const remembered = keyIn(persistent);
      return remembered === null ? null : { key: remembered, remember: true };
    },

    save({ key, remember }) {
      if (remember) {
        attempt(() => tab().removeItem(STORAGE_KEY), undefined);
        attempt(() => persistent().setItem(STORAGE_KEY, key), undefined);
      } else {
        attempt(() => tab().setItem(STORAGE_KEY, key), undefined);
      }
    },

    discard({ key, remember }) {
      const area = remember ? persistent : tab;
      if (keyIn(area) === key) {
        attempt(() => area().removeItem(STORAGE_KEY), undefined);
      }
    },

    clear() {
      attempt(() => persistent().removeItem(STORAGE_KEY), undefined);
      attempt(() => tab().removeItem(STORAGE_KEY), undefined);
    },

    subscribe(onChange) {
      function onStorage(event: StorageEvent) {
        if (
          event.storageArea === attempt(persistent, null) &&
          (event.key === STORAGE_KEY || event.key === null)
        ) {
          onChange(parseApiKey(event.newValue));
        }
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
