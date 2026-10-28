import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("toggles when its label is clicked", async () => {
    render(
      <Checkbox label="Remember me" hint="Keeps the key in this browser." />,
    );
    const checkbox = screen.getByRole("checkbox", { name: /remember me/i });
    expect(checkbox).not.toBeChecked();

    await userEvent.click(screen.getByText("Remember me"));

    expect(checkbox).toBeChecked();
    expect(
      screen.getByText("Keeps the key in this browser."),
    ).toBeInTheDocument();
  });
});
