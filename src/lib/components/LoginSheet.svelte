<script lang="ts">
import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import * as Sheet from "$lib/components/ui/sheet/index.js";
import { LoginError, createAccount, login } from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import type { Snippet } from "svelte";

let {
    title,
    loading = $bindable(false),
    sheetVisible = $bindable(false),
    showCreateAccount = false,
    children,
}: {
    title?: string;
    loading?: boolean;
    sheetVisible?: boolean;
    showCreateAccount?: boolean;
    children: Snippet;
} = $props();
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
    login(nsecOrHex)
        .catch((error) => {
            console.error("Error logging in: ", error);
            loginError = error;
            loading = false;
        })
        .finally(() => {
            loading = false;
        });
}

async function handleCreateAccount() {
    if (loading) return;
    loading = true;
    createAccount()
        .catch((error) => {
            console.error("Error creating account: ", error);
            loginError = error;
            loading = false;
        })
        .finally(() => {
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
    <Sheet.Content side="bottom" class="max-h-[90vh]">
        <KeyboardAvoidingView withSheet={true} bottomOffset={10} strategy="transform">
            <div class="overflow-y-auto pt-2 pb-32 px-1 relative min-h-[200px]">
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
            <div class="flex flex-col gap-0 w-full px-0 fixed bottom-0 left-0 right-0 bg-background border-t">
                {#if showCreateAccount}
                    <Button size="lg" variant="ghost" onclick={handleCreateAccount} disabled={loading} class="w-full h-fit text-base font-medium py-4 px-0">
                        Create a new Nostr key
                    </Button>
                {/if}
                <Button
                    size="lg"
                    variant="default"
                    onclick={handleLogin}
                    disabled={loading || nsecOrHex.length === 0}
                    class="text-base font-medium w-full h-fit mx-0 pt-4 pb-[calc(1rem+var(--sab))]"
                >Log in</Button>
            </div>
        </KeyboardAvoidingView>
    </Sheet.Content>
</Sheet.Root>
