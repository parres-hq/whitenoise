import { describe, expect, it, vi } from "vitest";
import { copyToClipboard } from "../clipboard";

// Mock Clipboard API
const mockWriteText = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
    writeText: mockWriteText,
}));

describe("copyToClipboard", () => {
    it("should copy text to clipboard", async () => {
        mockWriteText.mockResolvedValue(undefined);
        await copyToClipboard("test", "Failed to copy");
        expect(mockWriteText).toHaveBeenCalledWith("test");
    });
});
