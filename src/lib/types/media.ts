/**
 * Represents a media attachment in a message
 * @property {string} url - The URL to download the file
 * @property {string} mimeType - The MIME type of the file
 * @property {string} [blurhash] - The blurhash to show while the file is being loaded
 * @property {string} [dim] - The dimensions of the file in the form <width>x<height>
 */
export type MediaAttachment = {
    url: string;
    type: string;
    blurhashSvg?: string;
    dim?: string;
    alt?: string;
    width?: number;
    height?: number;
    decryptionNonceHex: string;
    fileHashOriginal: string;
};

export type MediaFileMap = Map<string, MediaFile>;

export type MediaFile = {
    media_file: {
        id: number;
        mls_group_id: { value: { vec: Uint8Array } };
        file_path: string;
        blossom_url: string | null;
        file_hash: string;
        nostr_key: string | null;
        created_at: number;
        file_metadata: {
            mime_type: string;
        } | null;
    };
    file_data: Uint8Array;
};
