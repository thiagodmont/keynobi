import type { StorybookConfig } from "storybook-solidjs-vite";
import { fileURLToPath } from "node:url";

const srcPath = fileURLToPath(new URL("../src", import.meta.url));

const config: StorybookConfig = {
  stories: ["../src/**/*.stories.@(ts|tsx)"],
  addons: ["@storybook/addon-docs", "@storybook/addon-a11y"],
  framework: {
    name: "storybook-solidjs-vite",
    options: {
      docgen: {
        savePropValueAsString: true,
        shouldExtractLiteralValuesFromEnum: true,
        propFilter: (prop) =>
          prop.parent === undefined || !prop.parent.fileName.includes("node_modules"),
      },
    },
  },
  docs: {
    autodocs: "tag",
  },
  async viteFinal(config) {
    const alias = config.resolve?.alias;

    return {
      ...config,
      resolve: {
        ...config.resolve,
        alias: Array.isArray(alias)
          ? [...alias, { find: "@", replacement: srcPath }]
          : { ...alias, "@": srcPath },
      },
      plugins: config.plugins?.filter((plugin) => {
        const name = (plugin as { name?: string } | undefined)?.name ?? "";
        return !name.includes("sentry");
      }),
    };
  },
};

export default config;
