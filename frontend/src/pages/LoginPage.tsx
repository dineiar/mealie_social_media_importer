import { type SubmitEvent, useEffect, useRef, useState } from "react";
import { useAuth } from "../auth/AuthProvider";
import { API_KEY_INPUT_PATTERN, parseApiKey } from "../auth/apiKey";
import { credentialOf } from "../auth/session";
import { Brand } from "../components/Brand";
import { Button } from "../components/Button";
import { Checkbox } from "../components/Checkbox";
import { Input } from "../components/Input";
import {
  AlertCircleIcon,
  ArrowRightIcon,
  EyeIcon,
  EyeOffIcon,
  KeyIcon,
  SpinnerIcon,
} from "../components/icons";

const CHECKING_LABEL = "Checking your key…";

export function LoginPage() {
  const { session, check } = useAuth();
  const seed = credentialOf(session);
  const [apiKey, setApiKey] = useState(seed?.key ?? "");
  const [remember, setRemember] = useState(seed?.remember ?? true);
  const [revealed, setRevealed] = useState(false);
  const keyField = useRef<HTMLInputElement>(null);

  const checking = session.kind === "checking";
  const rejected = session.kind === "rejected";
  const signedOut = session.kind === "signed_out";

  const [wasSignedOut, setWasSignedOut] = useState(signedOut);
  if (signedOut !== wasSignedOut) {
    setWasSignedOut(signedOut);
    if (signedOut) {
      setApiKey("");
      setRemember(true);
      setRevealed(false);
    }
  }

  useEffect(() => {
    if (rejected) {
      keyField.current?.focus();
    }
  }, [rejected]);

  function handleSubmit(event: SubmitEvent<HTMLFormElement>) {
    event.preventDefault();
    const key = parseApiKey(apiKey);
    if (key !== null) {
      check({ key, remember });
    }
  }

  return (
    <div className="login">
      <header className="login__header">
        <Brand />
      </header>
      <main className="login__main">
        <div className="card">
          <div className="card__head">
            <span className="card__icon" aria-hidden="true">
              <KeyIcon width={24} height={24} />
            </span>
            <div>
              <h1 className="card__title">Connect to Mealie</h1>
              <p className="card__text">
                Paste an API key from your Mealie account to start importing
                recipes from social media.
              </p>
            </div>
          </div>
          <p role="status" className="visually-hidden">
            {checking ? CHECKING_LABEL : ""}
          </p>
          <form
            className="login-form"
            method="post"
            aria-busy={checking || undefined}
            onSubmit={handleSubmit}
          >
            <div className="field">
              <label className="field__label" htmlFor="api-key">
                Mealie API key
              </label>
              <Input
                ref={keyField}
                mono
                id="api-key"
                name="api_key"
                type={revealed ? "text" : "password"}
                value={apiKey}
                placeholder="Paste your API key"
                autoComplete="off"
                spellCheck={false}
                required
                pattern={API_KEY_INPUT_PATTERN}
                title="An API key is one run of letters, digits and symbols, with no spaces."
                readOnly={checking}
                aria-invalid={rejected || undefined}
                aria-describedby={rejected ? "key-error key-hint" : "key-hint"}
                onChange={(event) => setApiKey(event.target.value)}
                action={{
                  label: revealed ? "Hide key" : "Show key",
                  icon: revealed ? EyeOffIcon : EyeIcon,
                  onClick: () => setRevealed(!revealed),
                }}
              />
              {rejected && (
                <div className="field__error" id="key-error" role="alert">
                  <AlertCircleIcon width={18} height={18} />
                  <span>
                    Mealie didn't accept this key. It may have been deleted, or
                    copied only in part.
                  </span>
                </div>
              )}
              <p className="field__hint" id="key-hint">
                In Mealie, open your profile › API Tokens and generate one.
              </p>
            </div>
            <Checkbox
              name="remember"
              label="Remember me"
              hint="Keeps the key in this browser until you log off."
              checked={remember}
              onChange={(event) => {
                if (!checking) {
                  setRemember(event.target.checked);
                }
              }}
            />
            <Button variant="primary" block type="submit" disabled={checking}>
              {checking ? (
                <>
                  <SpinnerIcon width={18} height={18} />
                  {CHECKING_LABEL}
                </>
              ) : (
                <>
                  Connect
                  <ArrowRightIcon width={18} height={18} />
                </>
              )}
            </Button>
          </form>
        </div>
      </main>
    </div>
  );
}
