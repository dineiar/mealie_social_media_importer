import type { ReactNode } from "react";
import { classNames } from "./classNames";
import { AlertCircleIcon } from "./icons";

export function Banner({
  tone,
  title,
  children,
  action,
}: {
  tone: "error" | "notice";
  title: string;
  children: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div
      className={classNames("banner", tone === "error" && "banner--error")}
      role={tone === "error" ? "alert" : "status"}
    >
      <div className="banner__inner">
        <AlertCircleIcon className="banner__icon" width={20} height={20} />
        <div className="banner__text">
          <p className="banner__title">{title}</p>
          <p className="banner__body">{children}</p>
        </div>
        {action}
      </div>
    </div>
  );
}
