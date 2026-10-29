import { describe, expect, it } from "vitest";
import { aMe, KEY, requireApiKey } from "../test/fixtures";
import {
  type Credential,
  credentialOf,
  initialSession,
  type Session,
  type SessionEvent,
  sessionReducer,
} from "./session";

const credential: Credential = { key: KEY, remember: true };
const me = aMe();

const signedOut: Session = { kind: "signed_out" };
const checking: Session = { kind: "checking", credential };
const rejected: Session = { kind: "rejected", credential };
const loggedIn: Session = { kind: "logged_in", credential, me };
const unreachable: Session = {
  kind: "unreachable",
  credential,
  cause: "mealie",
};

const lent: Credential = { key: KEY, remember: false };
const lentSessions = {
  checking: { kind: "checking", credential: lent },
  loggedIn: { kind: "logged_in", credential: lent, me },
  unreachable: { kind: "unreachable", credential: lent, cause: "backend" },
} satisfies Record<string, Session>;

const everyState = { signedOut, checking, rejected, loggedIn, unreachable };
const everyIdleState = { signedOut, rejected, loggedIn, unreachable };

function run(session: Session, ...events: SessionEvent[]): Session {
  return events.reduce(sessionReducer, session);
}

describe("initialSession", () => {
  it("starts signed out when no key is stored", () => {
    expect(initialSession(null)).toEqual(signedOut);
  });

  it("checks a stored key on load", () => {
    expect(initialSession(credential)).toEqual(checking);
  });
});

describe("sessionReducer", () => {
  it("walks a typed key to a logged in session", () => {
    expect(
      run(
        signedOut,
        { type: "check_requested", credential },
        { type: "verified", me },
      ),
    ).toEqual(loggedIn);
  });

  it("keeps the key in memory when Mealie rejects it, so the form can show it again", () => {
    expect(run(checking, { type: "rejected" })).toEqual(rejected);
  });

  it("keeps the key and says why when the check gets no answer", () => {
    expect(run(checking, { type: "unreachable", cause: "backend" })).toEqual({
      kind: "unreachable",
      credential,
      cause: "backend",
    });
  });

  it("lets the user enter another key after a rejection", () => {
    const other: Credential = { key: KEY, remember: false };

    expect(
      run(rejected, { type: "check_requested", credential: other }),
    ).toEqual({ kind: "checking", credential: other });
  });

  it("checks the same key again from an unreachable session", () => {
    expect(run(unreachable, { type: "check_requested", credential })).toEqual(
      checking,
    );
  });

  it.each(Object.entries(everyState))("logs off from %s", (_name, session) => {
    expect(run(session, { type: "logged_off" })).toEqual(signedOut);
  });

  it.each(Object.entries(everyIdleState))(
    "ignores an answer to a check that isn't running in %s",
    (_name, session) => {
      expect(run(session, { type: "verified", me })).toBe(session);
      expect(run(session, { type: "rejected" })).toBe(session);
      expect(run(session, { type: "unreachable", cause: "mealie" })).toBe(
        session,
      );
    },
  );

  it("does not start a second check while one runs or after login", () => {
    expect(run(checking, { type: "check_requested", credential })).toBe(
      checking,
    );
    expect(run(loggedIn, { type: "check_requested", credential })).toBe(
      loggedIn,
    );
  });
});

describe("a key changing in another tab", () => {
  const forgotten: SessionEvent = {
    type: "stored_key_changed",
    change: { kind: "forgotten" },
  };
  const remembered = (key: Credential["key"]): SessionEvent => ({
    type: "stored_key_changed",
    change: { kind: "remembered", key },
  });
  const otherKey = requireApiKey("another-key-9876543210");
  const withCredential = { checking, loggedIn, unreachable };

  it.each(Object.entries(withCredential))(
    "logs off %s when the remembered key is forgotten",
    (_name, session) => {
      expect(run(session, forgotten)).toEqual(signedOut);
    },
  );

  it.each(Object.entries(lentSessions))(
    "keeps %s when the key it holds in its own tab is forgotten elsewhere",
    (_name, session) => {
      expect(run(session, forgotten)).toBe(session);
    },
  );

  it.each([
    ["signed out", signedOut],
    ["rejected", rejected],
  ])("has nothing to follow when %s", (_name, session) => {
    expect(run(session, forgotten)).toBe(session);
    expect(run(session, remembered(otherKey))).toBe(session);
  });

  it.each(Object.entries(lentSessions))(
    "keeps %s and its answer when another tab remembers the same key",
    (_name, session) => {
      expect(run(session, remembered(KEY))).toEqual({
        ...session,
        credential: credential,
      });
    },
  );

  it.each(Object.entries(withCredential))(
    "checks the key another tab remembered in %s",
    (_name, session) => {
      expect(run(session, remembered(otherKey))).toEqual({
        kind: "checking",
        credential: { key: otherKey, remember: true },
      });
    },
  );

  it("changes nothing when another tab remembers the key this tab already remembers", () => {
    for (const session of Object.values(withCredential)) {
      expect(run(session, remembered(KEY))).toBe(session);
    }
  });
});

describe("credentialOf", () => {
  it("has no credential only when signed out", () => {
    expect(credentialOf(signedOut)).toBeNull();
    for (const session of [checking, rejected, loggedIn, unreachable]) {
      expect(credentialOf(session)).toBe(credential);
    }
  });
});
