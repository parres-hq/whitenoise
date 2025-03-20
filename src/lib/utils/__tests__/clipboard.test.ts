import * as ClipboardManager from "@tauri-apps/plugin-clipboard-manager";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { copyToClipboard } from "../clipboard";

vi.spyOn(ClipboardManager, "writeText").mockImplementation(async () => {
    return;
});
vi.spyOn(console, "error").mockImplementation(() => {});

describe("copyToClipboard", () => {
    it("should copy text to clipboard successfully", async () => {
        const result = await copyToClipboard("Hello", "Copy failed");
        expect(result).toBe(true);
    });

    it("should return false when copying fails", async () => {
        vi.spyOn(ClipboardManager, "writeText").mockRejectedValueOnce(new Error("Clipboard error"));

        const result = await copyToClipboard("Hello", "Copy failed");
        expect(result).toBe(false);
    });
});
