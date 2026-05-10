import { describe, expect, it } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { ControlStrip } from "./ControlStrip";

describe("ControlStrip", () => {
  it("renders children with test ids", () => {
    render(() => (
      <ControlStrip testId="strip" wrap>
        <button>Run</button>
      </ControlStrip>
    ));

    expect(screen.getByTestId("strip")).not.toBeNull();
    expect(screen.getByText("Run")).not.toBeNull();
  });
});
