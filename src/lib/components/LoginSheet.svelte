<script lang="ts">
import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
import Button from "$lib/components/ui/button/button.svelte";
import Input from "$lib/components/ui/input/input.svelte";
import * as Sheet from "$lib/components/ui/sheet/index.js";
import { LoginError, createAccount, login } from "$lib/stores/accounts";
import { readFromClipboard } from "$lib/utils/clipboard";
import Paste from "carbon-icons-svelte/lib/Paste.svelte";
import type { Snippet } from "svelte";
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
    <Sheet.Content side="bottom" class="pb-0 px-0">
        <KeyboardAvoidingView withSheet={true} bottomOffset={10} strategy="transform">
            <div class="overflow-y-auto pt-2 {showCreateAccount ? 'pb-32' : 'pb-20'} px-1 relative">
                <Sheet.Header class="text-left mb-8 px-6">
                    <Sheet.Title>{title ?? $t("login.signInWithNostrKey")}</Sheet.Title>
                    <Sheet.Description class="text-lg font-normal">
                        {$t("login.signInDescription")}
                    </Sheet.Description>
                </Sheet.Header>
                <div class="flex flex-col gap-x-4 relative px-6">
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
            <div class="flex flex-col gap-0 w-full px-0 fixed bottom-0 left-0 right-0 bg-background">
                {#if showCreateAccount}
                    <Button size="lg" variant="ghost" onclick={handleCreateAccount} disabled={loading} class="w-full h-fit text-base font-medium py-4 px-0 focus-visible:ring-0">
                        {$t("login.createNewNostrKey")}
                    </Button>
                {/if}
                <Button
                    size="lg"
                    variant="default"
                    onclick={handleLogin}
                    disabled={loading || nsecOrHex.length === 0}
                    class="text-base font-medium w-full h-fit mx-0 pt-4 pb-[calc(1rem+var(--sab))] focus-visible:ring-0"
                >
                    {$t("login.logIn")}
                </Button>
            </div>
        </KeyboardAvoidingView>
    </Sheet.Content>
</Sheet.Root>
