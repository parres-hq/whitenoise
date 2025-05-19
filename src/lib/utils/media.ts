import type { Message } from "$lib/types/chat";
import type { MediaAttachment } from "$lib/types/media";
import { readFile } from "@tauri-apps/plugin-fs";
import { blurhashToSVG } from "./blurhash";
import { findImetaTags } from "./tags";

export const ALLOWED_MIME_TYPES = ["image/*", "video/*", "audio/*", "application/pdf"];
export const MAX_VISIBLE_MEDIA_ATTACHMENTS = 3;

export function getMimeType(filePath: string): string {
    const extension = filePath.split(".").pop()?.toLowerCase();
    const mimeTypes: Record<string, string> = {
        jpg: "image/jpeg",
        jpeg: "image/jpeg",
        png: "image/png",
        gif: "image/gif",
        mp4: "video/mp4",
        mp3: "audio/mpeg",
        pdf: "application/pdf",
        // Add more as needed
    };
    return mimeTypes[extension || ""] || "application/octet-stream";
}

export function getTypeFromMimeType(mimeType: string): string {
    if (mimeType.startsWith("image/")) return "image";
    if (mimeType.startsWith("video/")) return "video";
    if (mimeType.startsWith("audio/")) return "audio";

    return mimeType.split("/")[1];
}

function mediaAttachmentFromTag(tag: string[]): MediaAttachment {
    const url = tag[1].split(" ")[1];
    const blurhash = tag[5]?.split(" ")[1];
    const mimeType = tag[2]?.split(" ")[1];
    const type = getTypeFromMimeType(mimeType);
    const isImage = mimeType?.startsWith("image/");
    const dim = tag[4]?.split(" ")[1].split("x");
    const width = dim ? Number.parseInt(dim[0]) : undefined;
    const height = dim ? Number.parseInt(dim[1]) : undefined;
    const blurhashSvg = blurhash && isImage ? blurhashToSVG(blurhash) : undefined;
    const fileHashOriginal = tag[6]?.split(" ")[1];
    const decryptionNonceHex = tag[7]?.split(" ")[1];
    return {
        url,
        type,
        blurhashSvg,
        fileHashOriginal,
        decryptionNonceHex,
        width,
        height,
    };
}

export function findMediaAttachments(message: Message): MediaAttachment[] {
    const imetaTags = findImetaTags(message.event);
    return imetaTags.map((tag) => mediaAttachmentFromTag(tag));
}

export function calculateGridColumns(visibleCount: number, hasHidden: boolean): number {
    if (visibleCount === 1) return 1;

    const visibleSquares = hasHidden ? MAX_VISIBLE_MEDIA_ATTACHMENTS : visibleCount;

    return visibleSquares < 6 && visibleSquares % 2 === 0 ? 2 : 3;
}

export function calculateVisibleAttachments(attachments: MediaAttachment[]) {
    const visibleCount =
        attachments.length > MAX_VISIBLE_MEDIA_ATTACHMENTS
            ? MAX_VISIBLE_MEDIA_ATTACHMENTS - 1
            : attachments.length;

    const visible = attachments.slice(0, visibleCount);
    const hiddenCount = attachments.length - visibleCount;

    return {
        visible,
        hiddenCount,
        hasHidden: hiddenCount > 0,
    };
}

export async function readLocalFile(filePath: string, mimeType: string) {
    try {
        const fileData = await readFile(filePath);
        const file = new File([fileData], filePath.split("/").pop() || "file", {
            type: mimeType,
        });
        return URL.createObjectURL(file);
    } catch (error) {
        console.error("Error reading file:", error);
        return null;
    }
}
