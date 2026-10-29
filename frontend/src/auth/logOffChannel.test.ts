import { afterEach, describe, expect, it, vi } from "vitest";
import { announceLogOff, listenForLogOff } from "./logOffChannel";

describe("the log off channel", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("tells a listener in another tab that the user logged off", async () => {
    const onLogOff = vi.fn();
    const stop = listenForLogOff(onLogOff);

    announceLogOff();

    await vi.waitFor(() => expect(onLogOff).toHaveBeenCalledTimes(1));
    stop();
  });

  it("stops telling a listener that has stopped", async () => {
    const stopped = vi.fn();
    const heard = vi.fn();
    listenForLogOff(stopped)();
    const stop = listenForLogOff(heard);

    announceLogOff();

    await vi.waitFor(() => expect(heard).toHaveBeenCalledTimes(1));
    expect(stopped).not.toHaveBeenCalled();
    stop();
  });

  it.each([
    [
      "has no BroadcastChannel",
      () => vi.stubGlobal("BroadcastChannel", undefined),
    ],
    [
      "refuses a channel",
      () =>
        vi.stubGlobal(
          "BroadcastChannel",
          class {
            constructor() {
              throw new DOMException("blocked", "SecurityError");
            }
          },
        ),
    ],
  ])("does nothing when the browser %s", (_name, withoutChannel) => {
    withoutChannel();
    const onLogOff = vi.fn();

    expect(() => announceLogOff()).not.toThrow();
    const stop = listenForLogOff(onLogOff);
    expect(() => stop()).not.toThrow();
    expect(onLogOff).not.toHaveBeenCalled();
  });
});
