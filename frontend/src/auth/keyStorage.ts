import { type ApiKey, parseApiKey } from "./apiKey";
import { attempt } from "./attempt";
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

export function createKeyStorage(
  persistent: StorageArea,
  tab: StorageArea,
): KeyStorage {
  const keyIn = (area: StorageArea) =>
    attempt(() => parseApiKey(area().getItem(STORAGE_KEY)), null);
  const store = (area: StorageArea, key: ApiKey) =>
    attempt(() => area().setItem(STORAGE_KEY, key), undefined);
  const forget = (area: StorageArea) =>
    attempt(() => area().removeItem(STORAGE_KEY), undefined);

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
        forget(tab);
        store(persistent, key);
      } else {
        store(tab, key);
      }
    },

    discard({ key, remember }) {
      const area = remember ? persistent : tab;
      if (keyIn(area) === key) {
        forget(area);
      }
    },

    clear() {
      forget(persistent);
      forget(tab);
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
