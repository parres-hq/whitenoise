import { describe, expect, it } from "vitest";
import { nostrWalletConnectUriError } from "../accounts";

describe("nostrWalletConnectUriError", () => {
    it("should validate a correct NWC URI with WSS relay", () => {
        const validUri =
            "nostr+walletconnect://b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4?relay=wss%3A%2F%2Frelay.damus.io&secret=71a8c14c1407c113601079c4302dab36460f0ccd0ad506f1f2dc73b5100e4f3c";
        const error = nostrWalletConnectUriError(validUri);
        expect(error).toBeNull();
    });

    it("should validate a correct NWC URI with WS relay", () => {
        const validUri = "nostr+walletconnect://pubkey?relay=ws://relay.example.com&secret=123";
        const error = nostrWalletConnectUriError(validUri);
        expect(error).toBeNull();
    });

    it("should validate URI with multiple relays", () => {
        const multiRelayUri =
            "nostr+walletconnect://pubkey?relay=wss://relay1.com&relay=ws://relay2.com&secret=123";
        const error = nostrWalletConnectUriError(multiRelayUri);
        expect(error).toBeNull();
    });

    it("should reject URI without correct prefix", () => {
        const invalidPrefixUri = "nostr://something";
        const error = nostrWalletConnectUriError(invalidPrefixUri);
        expect(error).toBe("Invalid URI format: must start with 'nostr+walletconnect://'");
    });

    it("should reject URI without relay parameter", () => {
        const noRelayUri = "nostr+walletconnect://pubkey?secret=123";
        const error = nostrWalletConnectUriError(noRelayUri);
        expect(error).toBe("Missing required 'relay' parameter");
    });

    it("should reject URI with non-WS/WSS relay", () => {
        const httpRelayUri = "nostr+walletconnect://pubkey?relay=http://relay.com&secret=123";
        const error = nostrWalletConnectUriError(httpRelayUri);
        expect(error).toBe("Relay must use either WSS or WS protocol");
    });

    it("should reject URI with mixed valid/invalid relay protocols", () => {
        const mixedRelayUri =
            "nostr+walletconnect://pubkey?relay=wss://relay1.com&relay=http://relay2.com&secret=123";
        const error = nostrWalletConnectUriError(mixedRelayUri);
        expect(error).toBe("Relay must use either WSS or WS protocol");
    });

    it("should reject URI with invalid relay URL format", () => {
        const invalidRelayUri = "nostr+walletconnect://pubkey?relay=not-a-url&secret=123";
        const error = nostrWalletConnectUriError(invalidRelayUri);
        expect(error).toBe("Invalid relay URL format");
    });

    it("should reject URI without secret parameter", () => {
        const noSecretUri = "nostr+walletconnect://pubkey?relay=wss://relay.com";
        const error = nostrWalletConnectUriError(noSecretUri);
        expect(error).toBe("Missing required 'secret' parameter");
    });

    it("should reject malformed URI", () => {
        const malformedUri = "not-even-a-uri";
        const error = nostrWalletConnectUriError(malformedUri);
        expect(error).toBe("Invalid URI format: must start with 'nostr+walletconnect://'");
    });

    it("should reject empty URI", () => {
        const emptyUri = "";
        const error = nostrWalletConnectUriError(emptyUri);
        expect(error).toBe("Invalid URI format: must start with 'nostr+walletconnect://'");
    });

    it("should validate URI with additional parameters", () => {
        const uriWithExtraParams =
            "nostr+walletconnect://pubkey?relay=wss://relay.com&secret=123&extra=param";
        const error = nostrWalletConnectUriError(uriWithExtraParams);
        expect(error).toBeNull();
    });

    it("should validate URI with encoded characters", () => {
        const encodedUri = "nostr+walletconnect://pubkey?relay=wss%3A%2F%2Frelay.com&secret=123";
        const error = nostrWalletConnectUriError(encodedUri);
        expect(error).toBeNull();
    });
});
