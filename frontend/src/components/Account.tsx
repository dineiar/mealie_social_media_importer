import type { Me } from "../api/types/Me";
import { Button } from "./Button";
import { LogOffIcon } from "./icons";

const NAMELESS_USER = "Mealie user";

function present(text: string | null): string | null {
  return text?.trim() || null;
}

function identityOf(me: Me) {
  const name = present(me.full_name);
  const username = present(me.username);
  const label = name ?? username ?? NAMELESS_USER;
  return {
    initial: Array.from(label)[0]?.toLocaleUpperCase() ?? "",
    name: name ?? (username ? null : NAMELESS_USER),
    handle: username && `@${username}`,
  };
}

export function Account({
  me,
  onLogOff,
}: {
  me: Me | null;
  onLogOff: () => void;
}) {
  const identity = me === null ? null : identityOf(me);
  return (
    <div className="account">
      {identity && (
        <>
          <div className="account__user">
            <span className="avatar" aria-hidden="true">
              {identity.initial}
            </span>
            <span className="account__names">
              {identity.name && (
                <span className="account__name">{identity.name}</span>
              )}
              {identity.handle && (
                <span className="account__handle">{identity.handle}</span>
              )}
            </span>
          </div>
          <span className="account__divider" aria-hidden="true" />
        </>
      )}
      <Button className="btn--logoff" onClick={onLogOff}>
        <LogOffIcon width={18} height={18} />
        Log off
      </Button>
    </div>
  );
}
