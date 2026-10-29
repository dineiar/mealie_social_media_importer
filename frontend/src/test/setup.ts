import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterAll, afterEach, beforeAll } from "vitest";
import {
  expectNoUnhandledRequests,
  recordUnhandledRequest,
  server,
} from "./server";

function dropWhatSafari16Lacks() {
  Reflect.deleteProperty(AbortSignal, "any");
}
dropWhatSafari16Lacks();

beforeAll(() =>
  server.listen({
    onUnhandledFrame: ({ frame }) => recordUnhandledRequest(frame),
  }),
);
afterEach(async () => {
  try {
    await expectNoUnhandledRequests();
  } finally {
    cleanup();
    server.resetHandlers();
    localStorage.clear();
    sessionStorage.clear();
  }
});
afterAll(async () => {
  try {
    await expectNoUnhandledRequests();
  } finally {
    server.close();
  }
});
