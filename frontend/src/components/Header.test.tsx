import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Header } from "./Header";

describe("Header", () => {
  it("shows the brand name and any children beside it", () => {
    render(
      <Header>
        <span>Ana</span>
      </Header>,
    );

    expect(screen.getByRole("banner")).toHaveTextContent(
      "Mealie Social Media Importer",
    );
    expect(screen.getByText("Ana")).toBeInTheDocument();
  });
});
