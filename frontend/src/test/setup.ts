import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterAll, afterEach, beforeAll } from "vitest";
import {
  expectNoUnhandledRequests,
  recordUnhandledRequest,
  server,
} from "./server";

// Safari only has AbortSignal.any from 17.4, and Vite's default build target
// includes Safari 16.4. Tests run without it so no code can depend on it.
Reflect.deleteProperty(AbortSignal, "any");

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
