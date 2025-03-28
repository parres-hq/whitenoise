import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { copyToClipboard, readFromClipboard } from "../clipboard";

// Mock Clipboard API
const mockWriteText = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
    writeText: mockWriteText,
    readText: vi.fn(),
}));

describe("copyToClipboard", () => {
    it("should copy text to clipboard", async () => {
        mockWriteText.mockResolvedValue(undefined);
        await copyToClipboard("test", "Failed to copy");
        expect(mockWriteText).toHaveBeenCalledWith("test");
    });

    it("should handle errors when copying fails", async () => {
        mockWriteText.mockRejectedValue(new Error("Copy failed"));
        const result = await copyToClipboard("test", "Failed to copy");
        expect(result).toBe(false);
        expect(mockWriteText).toHaveBeenCalledWith("test");
    });
});

describe("readFromClipboard", () => {
    beforeEach(() => {
        // Clear all mocks before each test
        vi.clearAllMocks();
    });

    it("should successfully read text from clipboard", async () => {
        // Mock successful clipboard read
        const mockText = "Test clipboard content";
        (readText as unknown as ReturnType<typeof vi.fn>).mockResolvedValue(mockText);

        const result = await readFromClipboard();
        expect(result).toBe(mockText);
        expect(readText).toHaveBeenCalledTimes(1);
    });

    it("should return null when clipboard read fails", async () => {
        // Mock clipboard read failure
        (readText as unknown as ReturnType<typeof vi.fn>).mockRejectedValue(
            new Error("Clipboard read failed")
        );

        const result = await readFromClipboard();
        expect(result).toBeNull();
        expect(readText).toHaveBeenCalledTimes(1);
    });

    it("should handle empty clipboard content", async () => {
        // Mock empty clipboard content
        (readText as unknown as ReturnType<typeof vi.fn>).mockResolvedValue("");

        const result = await readFromClipboard();
        expect(result).toBe("");
        expect(readText).toHaveBeenCalledTimes(1);
    });
});
