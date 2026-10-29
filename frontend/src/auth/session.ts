import type { Me } from "../api/types/Me";
import type { ApiKey } from "./apiKey";

export interface Credential {
  key: ApiKey;
  remember: boolean;
}

export type UnreachableCause = "mealie" | "backend";

export type StoredKeyChange =
  | { kind: "forgotten" }
  | { kind: "remembered"; key: ApiKey };

export type Session =
  | { kind: "signed_out" }
  | { kind: "checking"; credential: Credential }
  | { kind: "rejected"; credential: Credential }
  | { kind: "logged_in"; credential: Credential; me: Me }
  | { kind: "unreachable"; credential: Credential; cause: UnreachableCause };

export type SessionEvent =
  | { type: "check_requested"; credential: Credential }
  | { type: "verified"; me: Me }
  | { type: "rejected" }
  | { type: "unreachable"; cause: UnreachableCause }
  | { type: "logged_off" }
  | { type: "stored_key_changed"; change: StoredKeyChange };

export function initialSession(stored: Credential | null): Session {
  return stored === null
    ? { kind: "signed_out" }
    : { kind: "checking", credential: stored };
}

export function credentialOf(session: Session): Credential | null {
  return session.kind === "signed_out" ? null : session.credential;
}

function followStoredKey(session: Session, change: StoredKeyChange): Session {
  if (session.kind === "signed_out" || session.kind === "rejected") {
    return session;
  }
  const { credential } = session;
  if (change.kind === "forgotten") {
    return credential.remember ? { kind: "signed_out" } : session;
  }
  if (change.key !== credential.key) {
    return {
      kind: "checking",
      credential: { key: change.key, remember: true },
    };
  }
  return credential.remember
    ? session
    : { ...session, credential: { ...credential, remember: true } };
}

export function sessionReducer(session: Session, event: SessionEvent): Session {
  switch (event.type) {
    case "check_requested":
      return session.kind === "signed_out" ||
        session.kind === "rejected" ||
        session.kind === "unreachable"
        ? { kind: "checking", credential: event.credential }
        : session;
    case "verified":
      return session.kind === "checking"
        ? { kind: "logged_in", credential: session.credential, me: event.me }
        : session;
    case "rejected":
      return session.kind === "checking"
        ? { kind: "rejected", credential: session.credential }
        : session;
    case "unreachable":
      return session.kind === "checking"
        ? {
            kind: "unreachable",
            credential: session.credential,
            cause: event.cause,
          }
        : session;
    case "logged_off":
      return { kind: "signed_out" };
    case "stored_key_changed":
      return followStoredKey(session, event.change);
  }
}
