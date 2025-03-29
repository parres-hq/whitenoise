import { beforeEach, describe, expect, it, vi } from "vitest";
import { formatMessageTime, toUnixTimestamp, unixTimestamp } from "../time";

describe("time utils", () => {
    beforeEach(() => {
        // Mock the current date to be fixed for consistent testing
        vi.setSystemTime(new Date("2024-03-15T12:00:00Z"));
    });

    describe("formatMessageTime", () => {
        it("formats today's time correctly", () => {
            const now = Math.floor(Date.now() / 1000);
            const result = formatMessageTime(now);
            expect(result).toMatch(/^\d{1,2}:\d{2} [AP]M$/);
        });

        it("formats this week's time correctly", () => {
            const twoDaysAgo = Math.floor(Date.now() / 1000) - 2 * 24 * 60 * 60;
            const result = formatMessageTime(twoDaysAgo);
            expect(result).toMatch(/^(Mon|Tue|Wed|Thu|Fri|Sat|Sun)$/);
        });

        it("formats this year's time correctly", () => {
            const twoMonthsAgo = Math.floor(Date.now() / 1000) - 60 * 24 * 60 * 60;
            const result = formatMessageTime(twoMonthsAgo);
            expect(result).toMatch(/^[A-Za-z]{3} \d{1,2}$/);
        });

        it("formats old time correctly", () => {
            const oldTime = Math.floor(new Date("2023-01-01").getTime() / 1000);
            const result = formatMessageTime(oldTime);
            expect(result).toMatch(/^[A-Za-z]{3} \d{1,2}, \d{4}$/);
        });
    });

    describe("unixTimestamp", () => {
        it("returns current Unix timestamp in seconds", () => {
            const result = unixTimestamp();
            const expected = Math.floor(Date.now() / 1000);
            expect(result).toBe(expected);
        });
    });

    describe("toUnixTimestamp", () => {
        it("converts millisecond timestamp to seconds", () => {
            const msTimestamp = 1709913600000; // March 8, 2024
            const result = toUnixTimestamp(msTimestamp);
            expect(result).toBe(1709913600);
        });

        it("keeps second timestamp as is", () => {
            const secTimestamp = 1709913600; // March 8, 2024
            const result = toUnixTimestamp(secTimestamp);
            expect(result).toBe(1709913600);
        });

        it("handles edge cases", () => {
            expect(toUnixTimestamp(0)).toBe(0);
            expect(toUnixTimestamp(1000)).toBe(1000);
            expect(toUnixTimestamp(9999999999999)).toBe(9999999999);
        });
    });
});
