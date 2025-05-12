import type { Message } from "$lib/types/chat";
import { describe, expect, it } from "vitest";
import { findMediaAttachments, getMimeType, getTypeFromMimeType } from "../media";

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
                    ],
                    ["imeta", "url https://example.com/video.mp4", "mime video/mp4", "size 5678"],
                ],
            },
        } as Message;

        const attachments = findMediaAttachments(message);

        expect(attachments).toHaveLength(2);
        expect(attachments[0]).toEqual({
            url: "https://example.com/image.jpg",
            type: "image",
            blurhashSvg: expect.any(String),
        });
        expect(attachments[1]).toEqual({
            url: "https://example.com/video.mp4",
            type: "video",
            blurhashSvg: undefined,
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
});
