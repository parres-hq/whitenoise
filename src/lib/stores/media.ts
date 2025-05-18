import type { MediaFile } from "$lib/types/media";
import { readLocalFile } from "$lib/utils/media";
import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";

type MediaFilesMap = Map<string, string>;

export function createMediaStore() {
    const { subscribe, update } = writable<{
        mediaFilesMap: MediaFilesMap;
        fetchMediaFiles: (groupId: string) => Promise<MediaFile[]>;
        findMediaFile: (blossomUrl: string) => string | undefined;
    }>({
        mediaFilesMap: new Map(),
        fetchMediaFiles,
        findMediaFile,
    });

    /**
     * Fetches all media files for a group
     * @param {string} groupId - The ID of the group to fetch files for
     * @returns {Promise<MediaFile[]>} Array of media files
     */
    async function fetchMediaFiles(groupId: string): Promise<MediaFile[]> {
        try {
            const files = await invoke<MediaFile[]>("fetch_group_media_files", { groupId });

            const mediaFilesMap = new Map<string, string>();
            for (const file of files) {
                if (file.media_file.blossom_url && file.media_file.file_path) {
                    const localFile = await readLocalFile(
                        file.media_file.file_path,
                        file.media_file.file_metadata?.mime_type || ""
                    );
                    if (localFile) mediaFilesMap.set(file.media_file.blossom_url, localFile);
                }
            }

            update((state) => ({
                ...state,
                groupId,
                mediaFilesMap,
            }));

            return files;
        } catch (error) {
            console.error("Error fetching media files:", error);
            throw error;
        }
    }

    /**
     * Finds a media file by its blossom URL
     * @param {string} blossomUrl - The blossom URL to search for
     * @returns {MediaFile | undefined} The media file if found, undefined otherwise
     */
    function findMediaFile(blossomUrl: string): string | undefined {
        let result: string | undefined;
        subscribe((state) => {
            result = state.mediaFilesMap.get(blossomUrl);
        })();
        return result;
    }

    return {
        subscribe,
        fetchMediaFiles,
        findMediaFile,
        get mediaFilesMap() {
            let map: MediaFilesMap = new Map();
            subscribe((state) => {
                map = state.mediaFilesMap;
            })();
            return map;
        },
    };
}

export const media = createMediaStore();
