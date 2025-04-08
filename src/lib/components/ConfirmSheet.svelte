<script lang="ts">
import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
import Button from "$lib/components/ui/button/button.svelte";
import * as Sheet from "$lib/components/ui/sheet";
import type { Snippet } from "svelte";

interface ConfirmSheetProps {
    title: string;
    description: string;
    acceptText: string;
    cancelText: string;
    acceptFn: () => void;
    children: Snippet;
}

let { title, description, acceptText, cancelText, acceptFn, children }: ConfirmSheetProps =
    $props();

let open = $state(false);
</script>

<Sheet.Root bind:open openFocus={undefined}>
    <Sheet.Trigger>
        {@render children()}
    </Sheet.Trigger>
    <Sheet.Content side="bottom">
        <KeyboardAvoidingView withSheet={true}>
            <div class="overflow-y-auto pt-2 pb-16 px-1 relative">
                <Sheet.Header class="text-left mb-24 px-6">
                    <Sheet.Title>{title}</Sheet.Title>
                    <Sheet.Description class="text-base text-muted-foreground whitespace-pre-wrap">{@html description}</Sheet.Description>
                </Sheet.Header>
                <div class="flex flex-col gap-0 w-full px-0 fixed bottom-0 left-0 right-0 bg-background">
                    <Sheet.Close asChild>
                        <Button variant="ghost" size="lg" class="text-base font-medium w-full py-6 focus-visible:ring-0" onclick={() => open = false}>{cancelText}</Button>
                    </Sheet.Close>
                    <Button id="accept-button" variant="default" size="lg" class="text-base font-medium w-full pb-[calc(1.5rem+var(--sab))] pt-6 focus-visible:ring-0" onclick={acceptFn}>{acceptText}</Button>
                </div>
            </div>
        </KeyboardAvoidingView>
    </Sheet.Content>
</Sheet.Root>
