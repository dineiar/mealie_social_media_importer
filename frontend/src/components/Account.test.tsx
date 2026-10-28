import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { aMe } from "../test/fixtures";
import { Account } from "./Account";

describe("Account", () => {
  it("shows the avatar initial, full name and handle", () => {
    render(<Account me={aMe()} onLogOff={() => {}} />);

    expect(screen.getByText("A")).toBeInTheDocument();
    expect(screen.getByText("Ana Souza")).toBeInTheDocument();
    expect(screen.getByText("@ana")).toBeInTheDocument();
  });

  it("shows the name alone when Mealie has no username", () => {
    render(<Account me={aMe({ username: null })} onLogOff={() => {}} />);

    expect(screen.getByText("Ana Souza")).toBeInTheDocument();
    expect(screen.queryByText(/^@/)).not.toBeInTheDocument();
  });

  it("shows the handle alone, and its initial, when Mealie has no full name", () => {
    render(<Account me={aMe({ full_name: null })} onLogOff={() => {}} />);

    expect(screen.getByText("@ana")).toBeInTheDocument();
    expect(screen.getByText("A")).toBeInTheDocument();
    expect(screen.queryByText("Ana Souza")).not.toBeInTheDocument();
  });

  it("falls back to a generic name when Mealie has neither", () => {
    render(
      <Account
        me={aMe({ username: null, full_name: "  " })}
        onLogOff={() => {}}
      />,
    );

    expect(screen.getByText("Mealie user")).toBeInTheDocument();
    expect(screen.getByText("M")).toBeInTheDocument();
  });

  it("takes the initial from the first letter, not the first code unit", () => {
    render(
      <Account me={aMe({ full_name: "élan Vital" })} onLogOff={() => {}} />,
    );

    expect(screen.getByText("É")).toBeInTheDocument();
  });

  it("offers only Log off when the user isn't known", () => {
    render(<Account me={null} onLogOff={() => {}} />);

    expect(screen.getByRole("button", { name: "Log off" })).toBeInTheDocument();
    expect(screen.queryByText(/^@/)).not.toBeInTheDocument();
  });

  it("calls onLogOff", async () => {
    const onLogOff = vi.fn();
    render(<Account me={aMe()} onLogOff={onLogOff} />);

    await userEvent.click(screen.getByRole("button", { name: "Log off" }));

    expect(onLogOff).toHaveBeenCalledOnce();
  });
});
