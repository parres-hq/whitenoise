import type { StorybookConfig } from "@storybook/sveltekit";

const config: StorybookConfig = {
    stories: ["../src/**/*.stories.@(ts|svelte)"],
    addons: [
        "@storybook/addon-themes",
        "@storybook/addon-essentials",
        "@storybook/addon-svelte-csf",
    ],
    framework: {
        name: "@storybook/sveltekit",
        options: {},
    },
};
export default config;
