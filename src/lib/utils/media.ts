import type { MediaAttachment, Message } from "$lib/types/chat";
import { blurhashToSVG } from "./blurhash";
import { findImetaTags } from "./tags";

export const ALLOWED_MIME_TYPES = ["image/*", "video/*", "audio/*", "application/pdf"];

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
    const blurhashSvg = blurhash && isImage ? blurhashToSVG(blurhash) : undefined;
    return {
        url,
        type,
        blurhashSvg,
    };
}

export function findMediaAttachments(message: Message): MediaAttachment[] {
    const imetaTags = findImetaTags(message.event);
    return imetaTags.map((tag) => mediaAttachmentFromTag(tag));
}
