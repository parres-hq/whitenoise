import { describe, expect, it } from "vitest";
import type { MlsGroupId } from "../../types/nostr";
import { hexMlsGroupId } from "../group";

describe("hexMlsGroupId", () => {
    it("should convert a simple Uint8Array to hex string", () => {
        const input: MlsGroupId = { value: { vec: new Uint8Array([1, 2, 3, 4]) } };
        const expected = "01020304";
        expect(hexMlsGroupId(input)).toBe(expected);
    });

    it("should handle single byte values", () => {
        const input: MlsGroupId = { value: { vec: new Uint8Array([0, 255]) } };
        const expected = "00ff";
        expect(hexMlsGroupId(input)).toBe(expected);
    });

    it("should handle empty Uint8Array", () => {
        const input: MlsGroupId = { value: { vec: new Uint8Array([]) } };
        const expected = "";
        expect(hexMlsGroupId(input)).toBe(expected);
    });

    it("should handle values requiring padding", () => {
        const input: MlsGroupId = { value: { vec: new Uint8Array([0, 1, 10, 15]) } };
        const expected = "00010a0f";
        expect(hexMlsGroupId(input)).toBe(expected);
    });

    it("should handle larger values", () => {
        const input: MlsGroupId = { value: { vec: new Uint8Array([128, 192, 224, 240]) } };
        const expected = "80c0e0f0";
        expect(hexMlsGroupId(input)).toBe(expected);
    });
});
