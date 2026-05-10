import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

interface StoryIndexEntry {
  id: string;
  title: string;
  name: string;
  type: "story" | "docs";
}

interface StoryIndex {
  entries: Record<string, StoryIndexEntry>;
}

const A11Y_STORY_IDS = [
  "design-system-components-button--sizes-and-states",
  "design-system-components-input--search-and-states",
  "design-system-components-menulist--saved-filters",
  "design-system-components-popover--controlled",
  "design-system-components-dockedpanel--log-entry-detail",
  "design-system-components-alert--variants",
  "design-system-components-emptystate--densities",
  "design-system-foundations--overview",
];

async function gotoStory(page: Page, id: string): Promise<void> {
  await page.goto(`/iframe.html?id=${id}&viewMode=story`);
  await page.locator("#storybook-root").waitFor({ state: "visible" });
}

test("all component stories render without page errors", async ({ page, request }) => {
  const pageErrors: string[] = [];
  const consoleErrors: string[] = [];

  page.on("pageerror", (error) => pageErrors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") {
      consoleErrors.push(message.text());
    }
  });

  const response = await request.get("/index.json");
  expect(response.ok()).toBe(true);

  const index = (await response.json()) as StoryIndex;
  const componentStories = Object.values(index.entries)
    .filter((entry) => entry.type === "story")
    .filter((entry) => entry.title.startsWith("Design System/Components/"));

  expect(componentStories.length).toBeGreaterThan(20);

  for (const story of componentStories) {
    await test.step(story.id, async () => {
      await gotoStory(page, story.id);
      const childCount = await page
        .locator("#storybook-root")
        .evaluate((node) => node.childElementCount);
      expect(childCount).toBeGreaterThan(0);
    });
  }

  expect(pageErrors).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test.describe("selected stories", () => {
  for (const storyId of A11Y_STORY_IDS) {
    test(`${storyId} has no obvious a11y violations and is visible`, async ({ page }) => {
      await gotoStory(page, storyId);

      const root = page.locator("#storybook-root");
      await expect(root).toBeVisible();
      const screenshot = await root.screenshot();
      expect(screenshot.length).toBeGreaterThan(1_000);

      const results = await new AxeBuilder({ page }).include("#storybook-root").analyze();
      expect(
        results.violations.map((violation) => ({
          id: violation.id,
          impact: violation.impact,
          targets: violation.nodes.map((node) => node.target),
        }))
      ).toEqual([]);
    });
  }
});
