<script lang="ts">
import { mode } from "mode-watcher";
import { Toaster as Sonner, type ToasterProps as SonnerProps } from "svelte-sonner";

type $$Props = SonnerProps;
</script>

<Sonner
	theme={$mode}
	class="toaster group"
	toastOptions={{
		classes: {
			toast: "group toast group-[.toaster]:bg-background group-[.toaster]:text-foreground group-[.toaster]:border-border group-[.toaster]:shadow-lg rounded-none mt-[calc(var(--sat))]",
			description: "group-[.toast]:text-muted-foreground",
			actionButton: "group-[.toast]:bg-primary group-[.toast]:text-primary-foreground",
			cancelButton: "group-[.toast]:bg-muted group-[.toast]:text-muted-foreground",
		},
	}}
	{...$$restProps}
/>
