import { AuthProvider, useAuth } from "./auth/AuthProvider";
import { SignedInShell } from "./components/SignedInShell";
import { LoginPage } from "./pages/LoginPage";

function SessionGate() {
  const { session } = useAuth();
  switch (session.kind) {
    case "signed_out":
    case "checking":
    case "rejected":
      return <LoginPage />;
    case "logged_in":
    case "unreachable":
      return <SignedInShell session={session} />;
  }
}

export function App() {
  return (
    <AuthProvider>
      <SessionGate />
    </AuthProvider>
  );
}
