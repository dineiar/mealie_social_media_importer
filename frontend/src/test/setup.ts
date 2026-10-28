import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterAll, afterEach, beforeAll } from "vitest";
import {
  recordUnhandledRequest,
  server,
  takeUnhandledRequests,
} from "./server";

beforeAll(() =>
  server.listen({
    onUnhandledFrame: ({ frame }) => recordUnhandledRequest(frame),
  }),
);
afterEach(() => {
  cleanup();
  server.resetHandlers();
  localStorage.clear();
  sessionStorage.clear();
  const unhandled = takeUnhandledRequests();
  if (unhandled.length > 0) {
    throw new Error(`Requests without a handler:\n\n${unhandled.join("\n\n")}`);
  }
});
afterAll(() => server.close());
