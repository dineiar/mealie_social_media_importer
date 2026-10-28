import type { ComponentProps } from "react";
import { classNames } from "./classNames";

type ButtonVariant = "default" | "primary" | "dark" | "ghost";

type ButtonProps = Omit<ComponentProps<"button">, "aria-label"> & {
  variant?: ButtonVariant;
  block?: boolean;
} & (
    | { iconOnly: true; "aria-label": string }
    | { iconOnly?: false; "aria-label"?: string }
  );

export function Button({
  variant = "default",
  block = false,
  iconOnly = false,
  type = "button",
  className,
  ...props
}: ButtonProps) {
  return (
    <button
      type={type}
      className={classNames(
        "btn",
        variant !== "default" && `btn--${variant}`,
        iconOnly && "btn--icon",
        block && "btn--block",
        className,
      )}
      {...props}
    />
  );
}
