export type ApiKey = string & { readonly __brand: "ApiKey" };

const PRINTABLE_ASCII_RUN = "[!-~]+";
const API_KEY = new RegExp(`^${PRINTABLE_ASCII_RUN}$`);

export const API_KEY_INPUT_PATTERN = `\\s*${PRINTABLE_ASCII_RUN}\\s*`;

function isApiKey(value: string): value is ApiKey {
  return API_KEY.test(value);
}

export function parseApiKey(raw: string | null): ApiKey | null {
  const trimmed = raw?.trim() ?? "";
  return isApiKey(trimmed) ? trimmed : null;
}
