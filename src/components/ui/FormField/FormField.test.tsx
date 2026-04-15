import { describe, it, expect } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { FormField } from "./FormField";

describe("FormField", () => {
  it("renders the label text", () => {
    render(() => <FormField label="Email"><input /></FormField>);
    expect(screen.getByText("Email")).not.toBeNull();
  });

  it("renders description when provided", () => {
    render(() => (
      <FormField label="Name" description="Your full name"><input /></FormField>
    ));
    expect(screen.getByText("Your full name")).not.toBeNull();
  });

  it("does not render description when absent", () => {
    render(() => <FormField label="Name"><input /></FormField>);
    expect(screen.queryByText("Your full name")).toBeNull();
  });

  it("renders error message when provided", () => {
    render(() => (
      <FormField label="Email" error="Invalid email"><input /></FormField>
    ));
    expect(screen.getByText("Invalid email")).not.toBeNull();
  });

  it("does not render error element when error is absent", () => {
    render(() => <FormField label="Email"><input /></FormField>);
    expect(screen.queryByText("Invalid email")).toBeNull();
  });

  it("renders required asterisk when required is set", () => {
    render(() => <FormField label="Name" required><input /></FormField>);
    expect(screen.getByText("*")).not.toBeNull();
  });

  it("does not render asterisk when required is not set", () => {
    render(() => <FormField label="Name"><input /></FormField>);
    expect(screen.queryByText("*")).toBeNull();
  });

  it("renders children", () => {
    const { container } = render(() => (
      <FormField label="Name">
        <input type="text" data-testid="ctrl" />
      </FormField>
    ));
    expect(container.querySelector("[data-testid='ctrl']")).not.toBeNull();
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => (
      <FormField label="Name" class="my-field"><input /></FormField>
    ));
    expect(container.firstElementChild!.classList.contains("my-field")).toBe(true);
  });
});
