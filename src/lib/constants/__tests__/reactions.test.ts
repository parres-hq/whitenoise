import { describe, expect, it } from "vitest";
import { DEFAULT_REACTION_EMOJIS } from "../reactions";

describe("DEFAULT_REACTION_EMOJIS", () => {
    it("should be an array of ReactionEmoji objects", () => {
        expect(Array.isArray(DEFAULT_REACTION_EMOJIS)).toBe(true);
        expect(DEFAULT_REACTION_EMOJIS.length).toBeGreaterThan(0);
        expect(DEFAULT_REACTION_EMOJIS[0]).toHaveProperty("emoji");
        expect(DEFAULT_REACTION_EMOJIS[0]).toHaveProperty("name");
    });

    it("should have valid emoji characters", () => {
        for (const reaction of DEFAULT_REACTION_EMOJIS) {
            expect(typeof reaction.emoji).toBe("string");
            expect(reaction.emoji.length).toBeGreaterThan(0);
            // Check if the emoji is a valid Unicode emoji using a more inclusive range
            // This includes emoji modifiers, variation selectors, and other emoji-related characters
            expect(
                /[\u{1F300}-\u{1F9FF}\u{2600}-\u{26FF}\u{2700}-\u{27BF}\u{1F000}-\u{1F02F}\u{1F0A0}-\u{1F0FF}\u{1F100}-\u{1F64F}\u{1F680}-\u{1F6FF}\u{1F900}-\u{1F9FF}\u{1F1E6}-\u{1F1FF}]/u.test(
                    reaction.emoji
                )
            ).toBe(true);
        }
    });

    it("should have valid names", () => {
        for (const reaction of DEFAULT_REACTION_EMOJIS) {
            expect(typeof reaction.name).toBe("string");
            expect(reaction.name?.length).toBeGreaterThan(0);
            // Check if the name contains only lowercase letters and underscores
            if (reaction.name) {
                expect(/^[a-z_]+$/.test(reaction.name)).toBe(true);
            }
        }
    });

    it("should have unique emojis", () => {
        const emojis = DEFAULT_REACTION_EMOJIS.map((reaction) => reaction.emoji);
        const uniqueEmojis = new Set(emojis);
        expect(emojis.length).toBe(uniqueEmojis.size);
    });

    it("should have unique names", () => {
        const names = DEFAULT_REACTION_EMOJIS.map((reaction) => reaction.name).filter(
            (name): name is string => name !== undefined
        );
        const uniqueNames = new Set(names);
        expect(names.length).toBe(uniqueNames.size);
    });

    it("should contain specific expected reactions", () => {
        const expectedReactions = [
            { emoji: "❤️", name: "heart" },
            { emoji: "👍", name: "thumbs_up" },
            { emoji: "👎", name: "thumbs_down" },
            { emoji: "😂", name: "laugh" },
            { emoji: "🤔", name: "thinking" },
            { emoji: "🤙", name: "pura_vida" },
            { emoji: "😥", name: "sad" },
        ];

        for (const expected of expectedReactions) {
            expect(DEFAULT_REACTION_EMOJIS).toContainEqual(expected);
        }
    });
});
