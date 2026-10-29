import { describe, expect, it } from "vitest";
import { API_KEY_INPUT_PATTERN, parseApiKey } from "./apiKey";

const ZERO_WIDTH_SPACE = String.fromCharCode(0x200b);
const KEY_WITH_HIDDEN_CHARACTER = `abc${ZERO_WIDTH_SPACE}def`;

describe("parseApiKey", () => {
  it("trims the whitespace a paste brings along", () => {
    expect(parseApiKey("  abc.DEF-123_xyz\n")).toBe("abc.DEF-123_xyz");
  });

  it.each([
    ["null", null],
    ["an empty string", ""],
    ["whitespace only", "  \t\n"],
    ["a space inside the key", "abc def"],
    ["a character outside printable ASCII", KEY_WITH_HIDDEN_CHARACTER],
  ])("gives no key for %s", (_name, raw) => {
    expect(parseApiKey(raw)).toBeNull();
  });
});

describe("API_KEY_INPUT_PATTERN", () => {
  const pattern = new RegExp(`^(?:${API_KEY_INPUT_PATTERN})$`, "v");

  it("accepts what parseApiKey accepts and refuses what it refuses", () => {
    for (const raw of [
      "abc",
      "  abc.DEF-123_xyz \n",
      "abc def",
      KEY_WITH_HIDDEN_CHARACTER,
    ]) {
      expect(pattern.test(raw)).toBe(parseApiKey(raw) !== null);
    }
  });
});
