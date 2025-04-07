<script lang="ts">
import Avatar from "$lib/components/Avatar.svelte";
import Header from "$lib/components/Header.svelte";
import { Button } from "$lib/components/ui/button";
import { Input } from "$lib/components/ui/input";
import { Label } from "$lib/components/ui/label";
import { Textarea } from "$lib/components/ui/textarea";
import { activeAccount } from "$lib/stores/accounts";
import type { NMetadata } from "$lib/types/nostr";
import { invoke } from "@tauri-apps/api/core";
import Edit from "carbon-icons-svelte/lib/Edit.svelte";

let displayName = $state($activeAccount?.metadata?.display_name);
let name = $state($activeAccount?.metadata?.name);
let about = $state($activeAccount?.metadata?.about);
let website = $state($activeAccount?.metadata?.website);
let nostrAddress = $state($activeAccount?.metadata?.nip05);
let lightningAddress = $state($activeAccount?.metadata?.lud16);
let bannerImage = $state($activeAccount?.metadata?.banner);
let profilePicture = $state($activeAccount?.metadata?.picture);

let bannerFileInput: HTMLInputElement;
let profileFileInput: HTMLInputElement;

async function handleFileUpload(file: File, type: "banner" | "profile") {
    const reader = new FileReader();
    reader.onload = async (e) => {
        const data = new Uint8Array(e.target?.result as ArrayBuffer);
        try {
            const url = await invoke("upload_media", {
                file: {
                    filename: file.name,
                    mime_type: file.type,
                    data: Array.from(data),
                },
            });

            if (type === "banner") {
                bannerImage = url as string;
            } else {
                profilePicture = url as string;
            }
        } catch (error) {
            console.error("Failed to upload file:", error);
        }
    };
    reader.readAsArrayBuffer(file);
}

async function handleSave() {
    const newMetadata: NMetadata = {
        display_name: displayName,
        name: name,
        about: about,
        website: website,
        nip05: nostrAddress,
        lud16: lightningAddress,
        picture: profilePicture,
        banner: bannerImage,
    };

    await invoke("publish_metadata_event", { newMetadata });
}
</script>

<Header backLocation="/settings" title="Profile" />

<div class="pb-16 md:pb-6 flex flex-col gap-4">
    <div class="relative">
        {#if bannerImage}
            <img src={bannerImage} alt="Cover" class="w-full h-48 object-cover" />
        {:else}
            <img src="/images/static-placeholder.webp" alt="Cover" class="w-full h-48 object-cover" />
        {/if}

        <div class="absolute -bottom-16 left-1/2 -translate-x-1/2">
            <div class="relative border-8 border-background rounded-full">
                <Avatar
                    pubkey={$activeAccount!.pubkey}
                    picture={profilePicture}
                    pxSize={128}
                />
                <label class="absolute bottom-0 right-0 p-2 rounded-full bg-background border border-input hover:bg-accent cursor-pointer">
                    <input
                        type="file"
                        accept="image/*"
                        bind:this={profileFileInput}
                        class="hidden"
                        onchange={(e: Event) => {
                            const target = e.target as HTMLInputElement;
                            const file = target.files?.[0];
                            if (file) handleFileUpload(file, 'profile');
                        }}
                    />
                    <Edit size={16} />
                </label>
            </div>
        </div>

        <label class="absolute top-4 right-4 p-2 rounded-full bg-background border border-input hover:bg-accent cursor-pointer">
            <input
                type="file"
                accept="image/*"
                bind:this={bannerFileInput}
                class="hidden"
                onchange={(e: Event) => {
                    const target = e.target as HTMLInputElement;
                    const file = target.files?.[0];
                    if (file) handleFileUpload(file, 'banner');
                }}
            />
            <Edit size={16} />
        </label>
    </div>

    <div class="flex flex-col gap-6 mt-20 relative">
        <div class="flex flex-col gap-2 px-4">
            <Label for="displayName">Display Name</Label>
            <Input type="text" id="displayName" bind:value={displayName} />
        </div>

        <div class="flex flex-col gap-2 px-4">
            <Label for="name">Name</Label>
            <Input type="text" id="name" bind:value={name} />
        </div>

        <div class="flex flex-col gap-2 px-4">
            <Label for="about">About</Label>
            <Textarea
                id="about"
                bind:value={about}
            ></Textarea>
        </div>

        <div class="flex flex-col gap-2 px-4">
            <Label for="website">Website</Label>
            <Input type="url" id="website" bind:value={website} />
        </div>

        <div class="flex flex-col gap-2 px-4">
            <Label for="nostrAddress">Nostr Address (NIP-05)</Label>
            <Input type="text" id="nostrAddress" bind:value={nostrAddress} />
        </div>

        <div class="flex flex-col gap-2 px-4">
            <Label for="lightningAddress">Lightning Address</Label>
            <Input type="text" id="lightningAddress" bind:value={lightningAddress} />
        </div>
    </div>
    <div class="md:px-4">
        <Button size="lg" onclick={handleSave} class="text-base font-medium w-full h-fit fixed bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))] md:relative md:left-auto md:right-auto md:mt-6">Save Changes</Button>
    </div>
</div>
