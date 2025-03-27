<script lang="ts">
import type { Snippet } from "svelte";
import Loader from "./Loader.svelte";

type ButtonProps = {
    variant?: "default" | "secondary" | "destructive" | "outline" | "ghost" | "link";
    size?: "default" | "icon" | "sm" | "lg";
    icon?: string | null;
    loading?: boolean;
    disabled?: boolean;
    handleClick: () => void;
    children: Snippet;
};

let {
    variant = "default",
    size = "default",
    icon = null,
    loading = false,
    disabled = false,
    handleClick,
    children,
}: ButtonProps = $props();

const variantStyles = {
    default:
        "bg-primary-light dark:bg-primary-dark hover:bg-primary-light/90 dark:hover:bg-primary-dark/90 text-primary-foreground-light dark:text-primary-foreground-dark",
    secondary:
        "bg-secondary-light dark:bg-secondary-dark hover:bg-secondary-light/90 dark:hover:bg-secondary-dark/90 text-secondary-foreground-light dark:text-secondary-foreground-dark",
    destructive:
        "bg-destructive-light dark:bg-destructive-dark hover:bg-destructive-light/90 dark:hover:bg-destructive-dark/90 text-destructive-foreground-light dark:text-destructive-foreground-dark",
    outline:
        "border border-input bg-transparent hover:bg-accent-light dark:hover:bg-accent-dark text-primary-background-light dark:text-primary-background-dark",
    ghost: "bg-transparent hover:bg-accent-light dark:hover:bg-accent-dark text-primary-light dark:text-primary-dark",
    link: "bg-transparent hover:bg-accent-light dark:hover:bg-accent-dark text-primary-light dark:text-primary-dark",
};

const sizeStyles = {
    default: "text-base px-4 py-2",
    icon: "p-2",
    sm: "text-sm px-2 py-1",
    lg: "text-lg px-8 py-3 w-full",
};

const classes = $derived(`
    transition-colors flex flex-row items-center justify-center gap-2 font-medium
    ${variantStyles[variant]}
    ${sizeStyles[size]}
    ${disabled ? "opacity-80 cursor-not-allowed!" : ""}
`);
</script>

<button
    class={classes}
    disabled={disabled || loading}
    onclick={handleClick}
  >
    {#if loading}
      <Loader size={24} fullscreen={false}  />
    {:else if icon}
      <span>{icon}</span>
    {/if}

    {@render children()}

</button>
