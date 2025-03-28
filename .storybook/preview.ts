import { withThemeByClassName } from "@storybook/addon-themes";
import type { Preview } from "@storybook/svelte";
import { themes } from "@storybook/theming";
import "../src/app.css";

const preview: Preview = {
    decorators: [
        withThemeByClassName({
            themes: {
                light: "",
                dark: "dark",
            },
            defaultTheme: "light",
        }),
    ],
    parameters: {
        controls: {},
        backgrounds: { disable: true }, // Disable backgrounds addon
        themes: {
            default: "light",
            list: [
                { name: "light", class: "", color: "#f9f9f9" },
                { name: "dark", class: "dark", color: "#202320" },
            ],
        },
        docs: {
            theme: themes.light,
        },
    },
};

export default preview;
