import { describe, expect, it } from "vitest";
import { MLSCiphersuites } from "../mls";

describe("MLSCiphersuites", () => {
    it("should contain all expected ciphersuites", () => {
        // Test that all expected ciphersuites are present
        expect(MLSCiphersuites.get(0x0001)).toBe("MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519");
        expect(MLSCiphersuites.get(0x0002)).toBe("MLS_128_DHKEMP256_AES128GCM_SHA256_P256");
        expect(MLSCiphersuites.get(0x0003)).toBe(
            "MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519"
        );
        expect(MLSCiphersuites.get(0x0004)).toBe("MLS_256_DHKEMX448_AES256GCM_SHA512_Ed448");
        expect(MLSCiphersuites.get(0x0005)).toBe("MLS_256_DHKEMP521_AES256GCM_SHA512_P521");
        expect(MLSCiphersuites.get(0x0006)).toBe("MLS_256_DHKEMX448_CHACHA20POLY1305_SHA512_Ed448");
        expect(MLSCiphersuites.get(0x0007)).toBe("MLS_256_DHKEMP384_AES256GCM_SHA384_P384");
        expect(MLSCiphersuites.get(0x004d)).toBe("MLS_256_XWING_CHACHA20POLY1305_SHA256_Ed25519");
    });

    it("should have the correct number of ciphersuites", () => {
        expect(MLSCiphersuites.size).toBe(8);
    });

    it("should return undefined for unknown ciphersuite", () => {
        expect(MLSCiphersuites.get(0x9999)).toBeUndefined();
    });

    it("should have valid hex values as keys", () => {
        for (const [key] of MLSCiphersuites) {
            expect(typeof key).toBe("number");
            expect(key).toBeGreaterThan(0);
            expect(key).toBeLessThan(0x10000); // All keys should be 16-bit values
        }
    });
});
