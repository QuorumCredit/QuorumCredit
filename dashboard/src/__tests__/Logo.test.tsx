import React from "react";
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Logo } from "../Logo";

describe("Logo", () => {
  it("exposes an accessible name", () => {
    render(<Logo />);
    expect(screen.getByRole("img", { name: "QuorumCredit logo" })).toBeTruthy();
  });

  it("renders at the requested size and class", () => {
    const { container } = render(<Logo size={48} className="logo-large" />);
    const svg = container.querySelector("svg");
    expect(svg?.getAttribute("width")).toBe("48");
    expect(svg?.getAttribute("class")).toBe("logo-large");
  });
});
