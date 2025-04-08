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

<Sheet.Root bind:open>
    <Sheet.Trigger>
        {@render children()}
    </Sheet.Trigger>
    <Sheet.Content side="bottom" class="px-0">
        <KeyboardAvoidingView withSheet={true}>
            <Sheet.Header class="text-left mb-24 px-6">
                <Sheet.Title>{title}</Sheet.Title>
                <Sheet.Description class="text-base text-muted-foreground whitespace-pre-wrap">{@html description}</Sheet.Description>
            </Sheet.Header>
            <div class="flex flex-col gap-0 fixed bottom-0 left-0 right-0 mx-0 md:relative md:left-auto md:right-auto md:mt-6 focus-visible:ring-0">
                <Sheet.Close asChild>
                    <Button variant="ghost" size="lg" class="text-base font-medium w-full py-6" onclick={() => open = false}>{cancelText}</Button>
                </Sheet.Close>
                <Button id="accept-button" variant="default" size="lg" class="text-base font-medium w-full pb-[calc(1.5rem+var(--sab))] pt-6" onclick={acceptFn}>{acceptText}</Button>
            </div>
        </KeyboardAvoidingView>
    </Sheet.Content>
</Sheet.Root>
