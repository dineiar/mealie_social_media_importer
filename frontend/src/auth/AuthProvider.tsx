import { useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
} from "react";
import { type ApiClient, createApiClient } from "../api/client";
import { fetchMe } from "../api/me";
import { eventForFailure } from "./classifyFailure";
import { browserKeyStorage } from "./keyStorage";
import {
  type Credential,
  credentialOf,
  initialSession,
  type Session,
  sessionReducer,
} from "./session";

interface Auth {
  session: Session;
  api: ApiClient;
  check(credential: Credential): void;
  logOff(): void;
}

const AuthContext = createContext<Auth | null>(null);

export function useAuth(): Auth {
  const auth = useContext(AuthContext);
  if (auth === null) {
    throw new Error("useAuth needs an AuthProvider above it");
  }
  return auth;
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const [session, dispatch] = useReducer(sessionReducer, undefined, () =>
    initialSession(browserKeyStorage.read()),
  );

  const key = credentialOf(session)?.key ?? null;
  const api = useMemo(
    () =>
      createApiClient({
        getKey: () => key,
        onUnauthorized: () => {
          if (key !== null) {
            dispatch({ type: "rejected", key });
          }
        },
      }),
    [key],
  );

  const checking = session.kind === "checking";
  useEffect(() => {
    if (!checking) {
      return;
    }
    const controller = new AbortController();
    fetchMe(api, controller.signal).then(
      (me) => dispatch({ type: "verified", me }),
      (error: unknown) => {
        if (!controller.signal.aborted) {
          dispatch(eventForFailure(error));
        }
      },
    );
    return () => controller.abort();
  }, [checking, api]);

  useEffect(() => {
    switch (session.kind) {
      case "logged_in":
      case "unreachable":
        browserKeyStorage.save(session.credential);
        break;
      case "signed_out":
      case "rejected":
        browserKeyStorage.clear();
        queryClient.clear();
        break;
      case "checking":
        queryClient.clear();
        break;
    }
  }, [session, queryClient]);

  useEffect(
    () =>
      browserKeyStorage.subscribe((key) =>
        dispatch({ type: "stored_key_changed", key }),
      ),
    [],
  );

  const check = useCallback(
    (credential: Credential) =>
      dispatch({ type: "check_requested", credential }),
    [],
  );
  const logOff = useCallback(() => dispatch({ type: "logged_off" }), []);

  const auth = useMemo(
    () => ({ session, api, check, logOff }),
    [session, api, check, logOff],
  );
  return <AuthContext value={auth}>{children}</AuthContext>;
}
