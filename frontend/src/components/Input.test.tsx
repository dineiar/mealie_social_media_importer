import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Input } from "./Input";

function EyeIcon(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg {...props}>
      <title>Eye</title>
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

describe("Input", () => {
  it("accepts typing", async () => {
    render(<Input aria-label="Link" />);

    await userEvent.type(screen.getByRole("textbox", { name: "Link" }), "abc");

    expect(screen.getByRole("textbox", { name: "Link" })).toHaveValue("abc");
  });

  it("runs the trailing action from a button named after its label", async () => {
    const onClick = vi.fn();
    render(
      <Input
        aria-label="Key"
        type="password"
        action={{ label: "Show key", icon: EyeIcon, onClick }}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "Show key" }));

    expect(onClick).toHaveBeenCalledOnce();
  });

  it("hides the leading icon from assistive tech", () => {
    const { container } = render(<Input aria-label="Link" icon={EyeIcon} />);

    expect(container.querySelector("svg")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
  });
});
