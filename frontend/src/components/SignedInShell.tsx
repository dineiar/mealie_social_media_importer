import { useAuth } from "../auth/AuthProvider";
import type { Session, UnreachableCause } from "../auth/session";
import { Account } from "./Account";
import { Banner } from "./Banner";
import { Button } from "./Button";
import { Header } from "./Header";
import { RefreshIcon } from "./icons";

type SignedInSession = Extract<Session, { kind: "logged_in" | "unreachable" }>;

const UNREACHABLE_COPY: Record<
  UnreachableCause,
  { title: string; body: string }
> = {
  mealie: {
    title: "Mealie can't be reached",
    body: "Check that your Mealie server is running, then try again. Your key is kept.",
  },
  backend: {
    title: "The importer isn't responding",
    body: "Check that the importer's server is running, then try again. Your key is kept.",
  },
};

function UnreachableBanner({
  cause,
  onRetry,
}: {
  cause: UnreachableCause;
  onRetry: () => void;
}) {
  const { title, body } = UNREACHABLE_COPY[cause];
  return (
    <Banner
      tone="error"
      title={title}
      action={
        <Button onClick={onRetry}>
          <RefreshIcon width={16} height={16} />
          Try again
        </Button>
      }
    >
      {body}
    </Banner>
  );
}

export function SignedInShell({ session }: { session: SignedInSession }) {
  const { check, logOff } = useAuth();
  const me = session.kind === "logged_in" ? session.me : null;

  return (
    <>
      <Header>
        <Account me={me} onLogOff={logOff} />
      </Header>
      {session.kind === "unreachable" && (
        <UnreachableBanner
          cause={session.cause}
          onRetry={() => check(session.credential)}
        />
      )}
      {me && !me.ai.enabled && (
        <Banner tone="notice" title="AI is off in Mealie">
          Recipes can't be created until AI is set up in Mealie.
        </Banner>
      )}
      <main />
    </>
  );
}
