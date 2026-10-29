import type { ComponentProps } from "react";

type CheckboxProps = Omit<ComponentProps<"input">, "type" | "children"> & {
  label: string;
  hint?: string;
};

export function Checkbox({ label, hint, ...props }: CheckboxProps) {
  return (
    <label className="check">
      <input type="checkbox" {...props} />
      <span>
        <span className="check__label">{label}</span>
        {hint && <span className="check__hint">{hint}</span>}
      </span>
    </label>
  );
}
