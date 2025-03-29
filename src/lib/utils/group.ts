/**
 * Converts a Uint8Array MLS group ID to its hexadecimal string representation.
 * Each byte is converted to a two-digit hexadecimal number, padded with leading zeros if necessary.
 *
 * @param mlsGroupId - The MLS group ID as a Uint8Array
 * @returns A string containing the hexadecimal representation of the group ID
 * @example
 * const groupId = new Uint8Array([1, 2, 3, 4]);
 * const hexId = hexMlsGroupId(groupId); // Returns "01020304"
 */
export function hexMlsGroupId(mlsGroupId: Uint8Array): string {
    return Array.from(mlsGroupId, (byte) => byte.toString(16).padStart(2, "0")).join("");
}
