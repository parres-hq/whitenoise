import type { MediaFile } from "$lib/types/media";
import type { MediaAttachment } from "$lib/types/media";
import type { NGroup } from "$lib/types/nostr";
import { readLocalFile } from "$lib/utils/media";
import { invoke } from "@tauri-apps/api/core";
import { writable } from "svelte/store";

type MediaFilesMap = Map<string, string>;

export function createMediaStore() {
    const { subscribe, update } = writable<{
        mediaFilesMap: MediaFilesMap;
        isInitialLoading: boolean;
        fetchMediaFiles: (groupId: NGroup) => Promise<MediaFile[]>;
        findMediaFile: (blossomUrl: string) => string | undefined;
        downloadMedia: (group: NGroup, mediaAttachment: MediaAttachment) => Promise<string>;
        fetchMediaFile: (
            blossomUrl: string,
            filePath: string,
            fileMimeType: string
        ) => Promise<void>;
    }>({
        mediaFilesMap: new Map(),
        isInitialLoading: false,
        fetchMediaFiles,
        findMediaFile,
        downloadMedia,
        fetchMediaFile,
    });

    /**
     * Fetches all media files for a group
     * @param {NGroup} group - The group the media files belong to
     * @returns {Promise<MediaFile[]>} Array of media files
     */
    async function fetchMediaFiles(group: NGroup): Promise<MediaFile[]> {
        try {
            update((state) => ({ ...state, isInitialLoading: true }));
            const files = await invoke<MediaFile[]>("fetch_group_media_files", { group });

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
                mediaFilesMap,
                isInitialLoading: false,
            }));

            return files;
        } catch (error) {
            update((state) => ({ ...state, isInitialLoading: false }));
            console.error("Error fetching media files:", error);
            throw error;
        }
    }

    async function fetchMediaFile(blossomUrl: string, filePath: string, fileMimeType: string) {
        const localFile = await readLocalFile(filePath, fileMimeType);
        if (localFile) {
            update((state) => ({
                ...state,
                mediaFilesMap: new Map(state.mediaFilesMap).set(blossomUrl, localFile),
            }));
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

    /**
     * Downloads a media file and adds it to the store
     * @param {NGroup} group - The group the media belongs to
     * @param {MediaAttachment} mediaAttachment - The media attachment to download
     * @returns {Promise<string>} The local file path of the downloaded media
     */
    async function downloadMedia(group: NGroup, mediaAttachment: MediaAttachment): Promise<string> {
        try {
            const filePath = await invoke<string>("download_file", {
                group,
                decryptionNonceHex: mediaAttachment.decryptionNonceHex,
                mimeType: mediaAttachment.type,
                dimensions:
                    mediaAttachment.width && mediaAttachment.height
                        ? [mediaAttachment.width, mediaAttachment.height]
                        : undefined,
                fileHashOriginal: mediaAttachment.fileHashOriginal,
                blossomUrl: mediaAttachment.url,
            });
            return filePath;
        } catch (error) {
            console.error("Error downloading media:", error);
            throw error;
        }
    }

    return {
        subscribe,
        fetchMediaFiles,
        findMediaFile,
        downloadMedia,
        fetchMediaFile,
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
