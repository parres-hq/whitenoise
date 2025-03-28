import type { Preview } from "@storybook/svelte";
import { themes } from "@storybook/theming";
import "../src/app.css";

const preview: Preview = {
    parameters: {
        controls: {},
        backgrounds: {
            default: "dark",
            values: [
                {
                    name: "dark",
                    value: "#111827", // bg-gray-900
                },
            ],
        },
        docs: {
            theme: themes.dark,
        },
    },
};

export default preview;
