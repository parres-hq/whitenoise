/**
 * Formats a Unix timestamp into a human-readable string based on how recent the time is.
 * - For times today: Shows time (e.g., "3:45 PM")
 * - For times this week: Shows weekday (e.g., "Mon")
 * - For times this year: Shows month and day (e.g., "Mar 15")
 * - For older times: Shows full date (e.g., "Mar 15, 2023")
 *
 * @param timestamp - Unix timestamp in seconds
 * @returns Formatted string representation of the time
 */
export function formatMessageTime(timestamp: number): string {
    const date = new Date(timestamp * 1000);
    const now = new Date();
    const localDate = new Date(
        date.toLocaleString("en-US", { timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone })
    );

    const isToday = localDate.toDateString() === now.toDateString();
    const isThisWeek = now.getTime() - localDate.getTime() < 7 * 24 * 60 * 60 * 1000;
    const isThisYear = localDate.getFullYear() === now.getFullYear();

    if (isToday) {
        return localDate.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
    }
    if (isThisWeek) {
        return localDate.toLocaleDateString("en-US", { weekday: "short" });
    }
    if (isThisYear) {
        return localDate.toLocaleDateString("en-US", { month: "short", day: "numeric" });
    }
    return localDate.toLocaleDateString("en-US", {
        month: "short",
        day: "numeric",
        year: "numeric",
    });
}

/**
 * Returns the current Unix timestamp in seconds.
 * This is equivalent to Math.floor(Date.now() / 1000).
 *
 * @returns Current Unix timestamp in seconds
 */
export function unixTimestamp(): number {
    return Math.floor(Date.now() / 1000);
}

/**
 * Converts a timestamp to a Unix timestamp in seconds.
 * Handles both millisecond (13 digits) and second (10 digits) timestamps.
 *
 * @param timestamp - The timestamp to convert (in seconds or milliseconds)
 * @returns The Unix timestamp in seconds
 * @example
 * toUnixTimestamp(1709913600000) // returns 1709913600 (milliseconds to seconds)
 * toUnixTimestamp(1709913600)   // returns 1709913600 (already in seconds)
 */
export function toUnixTimestamp(timestamp: number): number {
    // Check if the timestamp is in milliseconds (13 digits) and convert to seconds if so
    if (timestamp.toString().length === 13) {
        return Math.floor(timestamp / 1000);
    }
    // If it's already in seconds (10 digits), return as is
    return Math.floor(timestamp);
}
