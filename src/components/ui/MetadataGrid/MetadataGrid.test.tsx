import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@solidjs/testing-library";
import { MetadataCell, MetadataGrid } from "./MetadataGrid";

describe("MetadataGrid", () => {
  it("renders labeled metadata cells", () => {
    render(() => (
      <MetadataGrid>
        <MetadataCell label="Tag" value="MainActivity" />
      </MetadataGrid>
    ));

    expect(screen.getByText("Tag")).not.toBeNull();
    expect(screen.getByText("MainActivity")).not.toBeNull();
  });

  it("supports clickable values", () => {
    const onClick = vi.fn();
    render(() => (
      <MetadataGrid>
        <MetadataCell label="Level" value="ERROR" onClick={onClick} />
      </MetadataGrid>
    ));

    fireEvent.click(screen.getByRole("button", { name: "ERROR" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
