import type { ComponentProps, ComponentType, SVGProps } from "react";
import { Button } from "./Button";
import { classNames } from "./classNames";

export type IconComponent = ComponentType<SVGProps<SVGSVGElement>>;

interface InputAction {
  label: string;
  icon: IconComponent;
  onClick: () => void;
}

type InputProps = ComponentProps<"input"> & {
  icon?: IconComponent;
  action?: InputAction;
  mono?: boolean;
};

export function Input({
  icon: Icon,
  action,
  mono = false,
  className,
  ...props
}: InputProps) {
  return (
    <div className="input-wrap">
      {Icon && (
        <Icon
          className="input-wrap__icon"
          width={20}
          height={20}
          aria-hidden="true"
        />
      )}
      <input
        className={classNames(
          "input",
          mono && "input--mono",
          action && "input--with-action",
          Icon && "input--with-icon",
          className,
        )}
        {...props}
      />
      {action && (
        <Button
          variant="ghost"
          iconOnly
          aria-label={action.label}
          className="input-wrap__action"
          onClick={action.onClick}
        >
          <action.icon width={20} height={20} aria-hidden="true" />
        </Button>
      )}
    </div>
  );
}
