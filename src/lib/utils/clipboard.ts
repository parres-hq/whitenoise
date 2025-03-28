import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";

/**
 * Copies text to the system clipboard using Tauri's clipboard manager.
 * @param text - The text to copy to the clipboard
 * @param errorMessage - Optional error message to display if the operation fails
 * @returns Promise<boolean> - Returns true if successful, false if the operation fails
 */
export async function copyToClipboard(text: string, errorMessage: string) {
    try {
        await writeText(text);
        return true;
    } catch (e) {
        console.error(e);
        return false;
    }
}

/**
 * Reads text from the system clipboard using Tauri's clipboard manager.
 * @returns Promise<string | null> - Returns the clipboard text if successful, null if the operation fails
 */
export async function readFromClipboard(): Promise<string | null> {
    try {
        return await readText();
    } catch (e) {
        console.error(e);
        return null;
    }
}
