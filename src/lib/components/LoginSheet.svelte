<script lang="ts">
import Sheet from "$lib/components/Sheet.svelte";
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import { LoginError, createAccount, login } from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import CloseLarge from "carbon-icons-svelte/lib/CloseLarge.svelte";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import type { Snippet } from "svelte";
import { onMount } from "svelte";
import { _ as t } from "svelte-i18n";

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
let platform = $state<"ios" | "android" | "desktop">("desktop");
let isClosing = $state(false);
let overlayVisible = $state(false);
let overlayOpacity = $state(0);

onMount(() => {
    if (typeof navigator !== "undefined") {
        if (/Android/i.test(navigator.userAgent)) {
            platform = "android";
        } else if (/iPhone|iPad|iPod/i.test(navigator.userAgent)) {
            platform = "ios";
        }
    }
});

async function handlePaste() {
    try {
        const text = await readFromClipboard();
        if (text) {
            nsecOrHex = text;
        } else {
            loginError = {
                name: $t("clipboard.error"),
                message: $t("clipboard.emptyTextError"),
            };
        }
    } catch (e) {
        loginError = {
            name: $t("clipboard.error"),
            message: $t("clipboard.readError"),
        };
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
            sheetVisible = false;
        });
}

$effect(() => {
    if (sheetVisible) {
        overlayVisible = true;
        // Fade in after next tick
        setTimeout(() => {
            overlayOpacity = 1;
        }, 0);
    } else if (!isClosing) {
        overlayOpacity = 0;
        setTimeout(() => {
            overlayVisible = false;
        }, 150);
    }
});
</script>

{@render children()}
<Sheet bind:open={sheetVisible}>
    {#snippet title()}{title ?? $t("login.signInWithNostrKey")}{/snippet}
    {#snippet description()}{$t("login.signInDescription")}{/snippet}
    <div class="flex flex-col flex-1 mx-4 md:mx-8 pt-4">
        <div class="flex flex-col relative">
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
    <div class="flex flex-col gap-2 w-full px-4 md:px-8 pb-8 bg-background">
        {#if showCreateAccount}
            <Button size="lg" variant="ghost" onclick={handleCreateAccount} disabled={loading} class="w-full text-base font-medium py-3 px-0 focus-visible:ring-0 disabled:cursor-not-allowed">
                {$t("login.createNewNostrKey")}
            </Button>
        {/if}
        <Button
            size="lg"
            variant="default"
            onclick={handleLogin}
            disabled={loading || nsecOrHex.length === 0}
            class="text-base font-medium w-full py-3 px-0 focus-visible:ring-0 disabled:cursor-not-allowed"
        >
            {$t("login.logIn")}
        </Button>
    </div>
</Sheet>
