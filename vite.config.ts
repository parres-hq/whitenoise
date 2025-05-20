import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

const excludes = [
    "node_modules/**",
    "dist/**",
    ".idea/**",
    ".git/**",
    ".cache/**",
    "src-tauri/**",
    "build/**",
    ".svelte-kit/**",
    "**/*.d.ts",
    "**/coverage/**",
    "vite.config.ts",
    "svelte.config.js",
    "postcss.config.js",
    "tailwind.config.js",
    "tsconfig.json",
    "**/*.svelte",
];

// https://vitejs.dev/config/
export default defineConfig(() => ({
    plugins: [sveltekit()],

    build: {
        rollupOptions: {
            external: ["blurhash"],
        },
    },

    // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
    //
    // 1. prevent vite from obscuring rust errors
    clearScreen: false,
    // 2. tauri expects a fixed port, fail if that port is not available
    server: {
        port: 1420,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                  protocol: "ws",
                  host,
                  port: 1421,
              }
            : undefined,
        watch: {
            // 3. tell vite to ignore watching `src-tauri`
            ignored: ["**/src-tauri/**"],
        },
    },

    // Vitest configuration
    test: {
        include: ["src/**/*.{test,spec}.{js,ts,jsx,tsx}"],
        exclude: [
            "node_modules/**",
            "dist/**",
            ".idea/**",
            ".git/**",
            ".cache/**",
            "src-tauri/**",
            "build/**",
            ".svelte-kit/**",
            "**/*.d.ts",
            "**/coverage/**",
            "vite.config.ts",
            "svelte.config.js",
            "postcss.config.js",
            "tailwind.config.js",
            "tsconfig.json",
            "**/*.svelte",
            "src/routes/**",
            "src/lib/utils.ts",
            "src/lib/components/ui/**",
            "tailwind.config.ts",
            ".storybook/**",
        ],
        globals: true,
        environment: "jsdom",
        coverage: {
            provider: "v8" as const,
            exclude: [
                "node_modules/**",
                "dist/**",
                ".idea/**",
                ".git/**",
                ".cache/**",
                "src-tauri/**",
                "build/**",
                ".svelte-kit/**",
                "**/*.d.ts",
                "**/coverage/**",
                "vite.config.ts",
                "svelte.config.js",
                "postcss.config.js",
                "tailwind.config.js",
                "tsconfig.json",
                "**/*.svelte",
                "src/routes/**",
                "src/lib/utils.ts",
                "src/lib/components/ui/**",
                "tailwind.config.ts",
                ".storybook/**",
            ],
        },
    },
}));
