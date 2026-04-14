import { describe, it, expect } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { Panel } from "./Panel";

describe("Panel", () => {
  it("renders children", () => {
    const { container } = render(() => (
      <Panel><span data-testid="body">content</span></Panel>
    ));
    expect(container.querySelector("[data-testid='body']")).not.toBeNull();
  });

  it("renders title when provided", () => {
    render(() => <Panel title="My Panel"><div /></Panel>);
    expect(screen.getByText("My Panel")).not.toBeNull();
  });

  it("does not render title element when title is absent", () => {
    const { container } = render(() => <Panel><div /></Panel>);
    expect(container.querySelector("header")).toBeNull();
  });

  it("renders headerActions slot", () => {
    const { container } = render(() => (
      <Panel title="T" headerActions={<button data-testid="btn">X</button>}>
        <div />
      </Panel>
    ));
    expect(container.querySelector("[data-testid='btn']")).not.toBeNull();
  });

  it("renders footer slot", () => {
    const { container } = render(() => (
      <Panel footer={<div data-testid="foot" />}><div /></Panel>
    ));
    expect(container.querySelector("[data-testid='foot']")).not.toBeNull();
  });

  it("does not render footer when absent", () => {
    const { container } = render(() => <Panel><div /></Panel>);
    expect(container.querySelector("footer")).toBeNull();
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => (
      <Panel class="my-panel"><div /></Panel>
    ));
    expect(container.firstElementChild!.classList.contains("my-panel")).toBe(true);
  });
});
