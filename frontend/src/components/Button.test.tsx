import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./Button";

describe("Button", () => {
  it("calls onClick", async () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>Save</Button>);

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(onClick).toHaveBeenCalledOnce();
  });

  it("does not submit a form unless it is a submit button", async () => {
    const onSubmit = vi.fn((event: React.SubmitEvent) =>
      event.preventDefault(),
    );
    render(
      <form onSubmit={onSubmit}>
        <Button>Plain</Button>
        <Button type="submit">Send</Button>
      </form>,
    );

    await userEvent.click(screen.getByRole("button", { name: "Plain" }));
    expect(onSubmit).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(onSubmit).toHaveBeenCalledOnce();
  });

  it("applies the design's modifier classes", () => {
    render(
      <>
        <Button variant="primary" block>
          Connect
        </Button>
        <Button variant="dark">Open</Button>
        <Button variant="ghost" iconOnly aria-label="Show key" />
      </>,
    );

    expect(screen.getByRole("button", { name: "Connect" })).toHaveClass(
      "btn",
      "btn--primary",
      "btn--block",
    );
    expect(screen.getByRole("button", { name: "Open" })).toHaveClass(
      "btn--dark",
    );
    expect(screen.getByRole("button", { name: "Show key" })).toHaveClass(
      "btn--ghost",
      "btn--icon",
    );
  });

  it("requires a label on an icon-only button", () => {
    // @ts-expect-error an icon-only button has no text, so it needs an aria-label
    render(<Button iconOnly />);
  });
});
