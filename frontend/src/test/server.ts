import { setupServer } from "msw/node";

export const server = setupServer();

const unhandledRequests: string[] = [];

export async function recordUnhandledRequest(frame: {
  getUnhandledMessage(): Promise<string>;
}): Promise<never> {
  const message = await frame.getUnhandledMessage();
  unhandledRequests.push(message);
  throw new Error(message);
}

export function takeUnhandledRequests(): string[] {
  return unhandledRequests.splice(0);
}
