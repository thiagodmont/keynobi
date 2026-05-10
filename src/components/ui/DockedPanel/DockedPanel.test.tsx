import { describe, expect, it } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { DockedPanel } from "./DockedPanel";

describe("DockedPanel", () => {
  it("renders title, subtitle, actions, and body", () => {
    render(() => (
      <DockedPanel title="JSON" subtitle="App: 12:00" actions={<button>Close</button>}>
        <pre>{"{}"}</pre>
      </DockedPanel>
    ));

    expect(screen.getByText("JSON")).not.toBeNull();
    expect(screen.getByText("App: 12:00")).not.toBeNull();
    expect(screen.getByText("Close")).not.toBeNull();
    expect(screen.getByText("{}")).not.toBeNull();
  });
});
