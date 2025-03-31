<script lang="ts">
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import * as Sheet from "$lib/components/ui/sheet/index.js";
import { LoginError, login } from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import type { Snippet } from "svelte";

let {
    title,
    loading = $bindable(false),
    sheetVisible = $bindable(false),
    children,
}: { title?: string; loading?: boolean; sheetVisible?: boolean; children: Snippet } = $props();
let nsecOrHex = $state("");
let loginError: LoginError | null = $state(null);

async function handlePaste() {
    try {
        const text = await readFromClipboard();
        if (text) {
            nsecOrHex = text;
        } else {
            loginError = { name: "ClipboardError", message: "No text found in clipboard" };
        }
    } catch (e) {
        loginError = { name: "ClipboardError", message: "Failed to read from clipboard" };
    }
}

async function handleLogin() {
    if (loading) return;
    loading = true;
    login(nsecOrHex).catch((error) => {
        console.error("Error logging in: ", error);
        loginError = error;
        loading = false;
    });
}
</script>

<Sheet.Root
    bind:open={sheetVisible}
    onOpenChange={(open) => {
        if (!open) {
            loginError = null;
            nsecOrHex = "";
        }
    }}
>
    <Sheet.Trigger>
        {@render children()}
    </Sheet.Trigger>
    <Sheet.Content side="bottom" class="pb-safe-bottom">
        <div class="overflow-y-auto pb-12 px-1 relative">
            <Sheet.Header class="text-left mb-8">
                <Sheet.Title>{title ?? "Sign in with your Nostr key"}</Sheet.Title>
                <Sheet.Description class="text-lg font-normal">
                    Your key will be encrypted and stored only on your device.
                </Sheet.Description>
            </Sheet.Header>
            <div class="flex flex-col gap-x-4 relative">
                <div class="flex flex-row gap-2">
                    <Input
                        bind:value={nsecOrHex}
                        type="password"
                        autofocus={false}
                        placeholder="nsec1..."
                        autocomplete="off"
                        autocapitalize="off"
                        autocorrect="off"
                        class="mb-1"
                    />
                    <Button variant="outline" size="icon" onclick={handlePaste} class="shrink-0">
                        <Paste size={16}/>
                    </Button>
                </div>
                <div class="h-8 text-sm text-destructive ml-1">
                    {loginError?.message}
                </div>
            </div>
        </div>
            <Button
                size="lg"
                variant="default"
                onclick={handleLogin}
                disabled={loading || nsecOrHex.length === 0}
                class="text-base font-medium w-full h-fit absolute bottom-0 left-0 right-0 mx-0 pt-4 pb-[calc(1rem+var(--sab))]"
            >Log in</Button>
    </Sheet.Content>
</Sheet.Root>
