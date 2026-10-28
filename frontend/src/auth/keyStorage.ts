import { parseApiKey } from "./apiKey";
import type { Credential } from "./session";

const STORAGE_KEY = "mealie-importer.api-key";

type StorageArea = () => Storage;

export interface KeyStorage {
  read(): Credential | null;
  save(credential: Credential): void;
  clear(): void;
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
  };
}

export const browserKeyStorage = createKeyStorage(
  () => window.localStorage,
  () => window.sessionStorage,
);
