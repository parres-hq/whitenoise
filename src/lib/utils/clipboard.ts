import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function copyToClipboard(text: string, errorMessage: string) {
    try {
        await writeText(text);
        return true;
    } catch (e) {
        console.error(e);
        return false;
    }
}
