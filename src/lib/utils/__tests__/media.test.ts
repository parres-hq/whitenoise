import type { Message } from "$lib/types/chat";
import { describe, expect, it } from "vitest";
import {
    calculateGridColumns,
    calculateVisibleAttachments,
    findMediaAttachments,
    getMimeType,
    getTypeFromMimeType,
} from "../media";

describe("getMimeType", () => {
    it("returns image mime type for jpg", () => {
        expect(getMimeType("test.jpg")).toBe("image/jpeg");
    });
    it("returns image mime type for jpeg", () => {
        expect(getMimeType("test.jpeg")).toBe("image/jpeg");
    });
    it("returns image mime type for png", () => {
        expect(getMimeType("test.png")).toBe("image/png");
    });
    it("returns image mime type for gif", () => {
        expect(getMimeType("test.gif")).toBe("image/gif");
    });
    it("returns video mime type for mp4", () => {
        expect(getMimeType("test.mp4")).toBe("video/mp4");
    });

    it("returns audio mime type for mp3", () => {
        expect(getMimeType("test.mp3")).toBe("audio/mpeg");
    });

    it("returns application mime type for pdf", () => {
        expect(getMimeType("test.pdf")).toBe("application/pdf");
    });

    it("returns octet-stream for unknown extensions", () => {
        expect(getMimeType("test.xyz")).toBe("application/octet-stream");
    });

    it("returns octet-stream for files without extensions", () => {
        expect(getMimeType("test")).toBe("application/octet-stream");
    });
});

describe("getTypeFromMimeType", () => {
    it("should return correct type for image mime types", () => {
        expect(getTypeFromMimeType("image/jpeg")).toBe("image");
        expect(getTypeFromMimeType("image/png")).toBe("image");
    });

    it("should return correct type for video mime types", () => {
        expect(getTypeFromMimeType("video/mp4")).toBe("video");
        expect(getTypeFromMimeType("video/quicktime")).toBe("video");
    });

    it("should return correct type for audio mime types", () => {
        expect(getTypeFromMimeType("audio/mpeg")).toBe("audio");
        expect(getTypeFromMimeType("audio/wav")).toBe("audio");
    });

    it("should return subtype for other mime types", () => {
        expect(getTypeFromMimeType("application/pdf")).toBe("pdf");
    });
});

describe("findMediaAttachments", () => {
    it("should extract media attachments from message with imeta tags", () => {
        const message: Message = {
            event: {
                tags: [
                    [
                        "imeta",
                        "url https://example.com/image.jpg",
                        "mime image/jpeg",
                        "size 1234",
                        "dim 800x600",
                        "blurhash LGI4eB~C~BR5W7I9x[-;RQyDM{Rj",
                        "d abc123",
                        "x def456",
                    ],
                    [
                        "imeta",
                        "url https://example.com/video.mp4",
                        "mime video/mp4",
                        "size 5678",
                        "dim 400x500",
                        "blurhash LGI4eB~C~BR5W7I9x[-;RQyDM{Rj",
                        "d ghi789",
                        "x jkl012",
                    ],
                ],
            },
        } as Message;

        const attachments = findMediaAttachments(message);

        expect(attachments).toHaveLength(2);
        expect(attachments[0]).toEqual({
            url: "https://example.com/image.jpg",
            type: "image",
            blurhashSvg: expect.any(String),
            decryptionNonceHex: "def456",
            fileHashOriginal: "abc123",
            width: 800,
            height: 600,
        });
        expect(attachments[1]).toEqual({
            url: "https://example.com/video.mp4",
            type: "video",
            blurhashSvg: undefined,
            decryptionNonceHex: "jkl012",
            fileHashOriginal: "ghi789",
            width: 400,
            height: 500,
        });
    });

    it("should return empty array for message without imeta tags", () => {
        const message: Message = {
            event: {
                tags: [["p", "some-other-tag"]],
            },
        } as Message;

        const attachments = findMediaAttachments(message);
        expect(attachments).toHaveLength(0);
    });

    describe("calculateGridColumns", () => {
        it("should return 1 column for single visible attachment", () => {
            expect(calculateGridColumns(1, false)).toBe(1);
        });

        it("should return 2 columns for even number of visible attachments less than 6", () => {
            expect(calculateGridColumns(2, false)).toBe(2);
            expect(calculateGridColumns(4, false)).toBe(2);
        });

        it("should return 3 columns for odd number of visible attachments less than 6", () => {
            expect(calculateGridColumns(3, false)).toBe(3);
            expect(calculateGridColumns(5, false)).toBe(3);
        });

        it("should return 3 columns when there are hidden attachments", () => {
            expect(calculateGridColumns(2, true)).toBe(3);
            expect(calculateGridColumns(4, true)).toBe(3);
        });
    });

    describe("calculateVisibleAttachments", () => {
        const mockAttachments = [
            {
                url: "url1",
                type: "image",
                decryptionNonceHex: "abc123",
                fileHashOriginal: "def456",
                width: undefined,
                height: undefined,
            },
            {
                url: "url2",
                type: "image",
                decryptionNonceHex: "ghi789",
                fileHashOriginal: "jkl012",
                width: undefined,
                height: undefined,
            },
            {
                url: "url3",
                type: "image",
                decryptionNonceHex: "mno345",
                fileHashOriginal: "pqr678",
                width: undefined,
                height: undefined,
            },
            {
                url: "url4",
                type: "image",
                decryptionNonceHex: "stu901",
                fileHashOriginal: "vwx234",
                width: undefined,
                height: undefined,
            },
            {
                url: "url5",
                type: "image",
                decryptionNonceHex: "yz567",
                fileHashOriginal: "abc890",
                width: undefined,
                height: undefined,
            },
        ];

        it("should show all attachments when count is less than max", () => {
            const attachments = mockAttachments.slice(0, 2);
            const result = calculateVisibleAttachments(attachments);

            expect(result.visible).toHaveLength(2);
            expect(result.hiddenCount).toBe(0);
            expect(result.hasHidden).toBe(false);
        });

        it("should show max - 1 when count exceeds max", () => {
            const result = calculateVisibleAttachments(mockAttachments);

            expect(result.visible).toHaveLength(2);
            expect(result.hiddenCount).toBe(3);
            expect(result.hasHidden).toBe(true);
        });

        it("should handle empty attachments array", () => {
            const result = calculateVisibleAttachments([]);

            expect(result.visible).toHaveLength(0);
            expect(result.hiddenCount).toBe(0);
            expect(result.hasHidden).toBe(false);
        });

        it("should handle exactly max attachments", () => {
            const attachments = mockAttachments.slice(0, 3);
            const result = calculateVisibleAttachments(attachments);

            expect(result.visible).toHaveLength(3);
            expect(result.hiddenCount).toBe(0);
            expect(result.hasHidden).toBe(false);
        });
    });
});
