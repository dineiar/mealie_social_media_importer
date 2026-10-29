import { setupServer } from "msw/node";

export const server = setupServer();

const unhandledRequests: Promise<string>[] = [];

export function recordUnhandledRequest(frame: {
  getUnhandledMessage(): Promise<string>;
}): Promise<never> {
  const message = frame.getUnhandledMessage();
  unhandledRequests.push(message);
  return message.then((text) => {
    throw new Error(text);
  });
}

const nextTask = () => new Promise<void>((resolve) => setTimeout(resolve));

// A fetch the test never awaited reaches MSW a few ticks after the test body ends.
export async function expectNoUnhandledRequests(): Promise<void> {
  await nextTask();
  const unhandled = await Promise.all(unhandledRequests.splice(0));
  if (unhandled.length > 0) {
    throw new Error(`Requests without a handler:\n\n${unhandled.join("\n\n")}`);
  }
}
