import { describe, expect, it } from "vitest";
import { blurhashToSVG } from "../blurhash";

describe("blurhashToSVG", () => {
    it("converts a valid blurhash to an SVG data URL", () => {
        const blurhash = "LEHV6nWB2yk8pyo0adR*.7kCMdnj";
        const result = blurhashToSVG(blurhash);
        expect(result).toMatch(/^data:image\/svg\+xml;base64,/);
        const base64Part = result.replace("data:image/svg+xml;base64,", "");
        const decoded = atob(base64Part);
        expect(decoded).toContain("<svg");
        expect(decoded).toContain("</svg>");
    });

    it("uses default dimensions when not specified", () => {
        const blurhash = "LEHV6nWB2yk8pyo0adR*.7kCMdnj";
        const result = blurhashToSVG(blurhash);
        const base64Part = result.replace("data:image/svg+xml;base64,", "");
        const decoded = atob(base64Part);
        expect(decoded).toContain('width="64"');
        expect(decoded).toContain('height="64"');
    });

    it("uses custom dimensions when specified", () => {
        const blurhash = "LEHV6nWB2yk8pyo0adR*.7kCMdnj";
        const width = 32;
        const height = 32;
        const result = blurhashToSVG(blurhash, width, height);
        const base64Part = result.replace("data:image/svg+xml;base64,", "");
        const decoded = atob(base64Part);
        expect(decoded).toContain(`width="${width}"`);
        expect(decoded).toContain(`height="${height}"`);
    });

    it("generates SVG with correct number of rect elements", () => {
        const blurhash = "LEHV6nWB2yk8pyo0adR*.7kCMdnj";
        const width = 4;
        const height = 4;
        const result = blurhashToSVG(blurhash, width, height);
        const base64Part = result.replace("data:image/svg+xml;base64,", "");
        const decoded = atob(base64Part);
        const rectCount = (decoded.match(/<rect/g) || []).length;
        expect(rectCount).toBe(width * height);
    });
});
