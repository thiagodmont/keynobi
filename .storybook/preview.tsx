import type { Preview } from "storybook-solidjs-vite";
import { createJSXDecorator } from "storybook-solidjs-vite";
import "../src/styles/global.css";
import "./preview.css";

const withKeynobiSurface = createJSXDecorator((Story) => (
  <main class="storybook-keynobi-surface">
    <Story />
  </main>
));

const preview: Preview = {
  decorators: [withKeynobiSurface],
  parameters: {
    actions: { argTypesRegex: "^on[A-Z].*" },
    controls: {
      expanded: true,
      matchers: {
        color: /(background|color)$/i,
        date: /Date$/i,
      },
    },
    docs: {
      toc: true,
    },
    backgrounds: {
      default: "Keynobi",
      values: [
        { name: "Keynobi", value: "#1e1e1e" },
        { name: "Panel", value: "#252526" },
      ],
    },
  },
};

export default preview;
