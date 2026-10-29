import { describe, expect, it } from "vitest";
import { aMe } from "../test/fixtures";
import { parseMe } from "./parseMe";

describe("parseMe", () => {
  it("returns a well formed answer", () => {
    expect(parseMe(aMe())).toEqual(aMe());
  });

  it("accepts a user without a username or a full name", () => {
    const nameless = aMe({ username: null, full_name: null });

    expect(parseMe(nameless)).toEqual(nameless);
  });

  it("drops fields it doesn't know", () => {
    expect(parseMe({ ...aMe(), role: "admin" })).toEqual(aMe());
  });

  it.each([
    ["undefined", undefined],
    ["null", null],
    ["a string", "ok"],
    ["a list", [aMe()]],
    ["an empty object", {}],
    ["a missing id", { ...aMe(), id: undefined }],
    ["a numeric id", { ...aMe(), id: 7 }],
    ["a missing username", { ...aMe(), username: undefined }],
    ["a numeric full name", { ...aMe(), full_name: 7 }],
    ["a missing group", { ...aMe(), group_slug: undefined }],
    ["no AI status", { ...aMe(), ai: undefined }],
    ["a null AI status", { ...aMe(), ai: null }],
    [
      "a string AI flag",
      { ...aMe(), ai: { enabled: "yes", image_enabled: true } },
    ],
    ["a missing image flag", { ...aMe(), ai: { enabled: true } }],
  ])("throws on %s", (_name, body) => {
    expect(() => parseMe(body)).toThrow(TypeError);
  });
});
