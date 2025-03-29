import { getContext, setContext } from "svelte";

/**
 * Represents a toast notification in the system
 */
export type Toast = {
    id: string;
    title: string;
    message: string;
    type: "error" | "success" | "info";
};

/**
 * Manages the state and lifecycle of toast notifications in the application
 * Uses Svelte's context system for state management
 */
export class ToastState {
    /** Array of active toast notifications */
    toasts = $state<Toast[]>([]);
    /** Map of toast IDs to their timeout handles */
    toastTimeoutMap = new Map<string, number>();

    /**
     * Adds a new toast notification to the system
     * @param title - The title of the toast
     * @param message - The main message content
     * @param type - The visual style of the toast
     * @param durationMs - Duration in milliseconds before auto-dismissal
     */
    add(title: string, message: string, type: "error" | "success" | "info", durationMs = 10_000) {
        const id = crypto.randomUUID();
        this.toasts.push({ id, title, message, type });

        this.toastTimeoutMap.set(
            id,
            Number(
                setTimeout(() => {
                    this.remove(id);
                }, durationMs)
            )
        );
    }

    /**
     * Removes a toast notification by its ID
     * @param {string} id - The unique identifier of the toast to remove
     */
    remove(id: string) {
        const timeout = this.toastTimeoutMap.get(id);
        if (timeout) {
            clearTimeout(timeout);
            this.toastTimeoutMap.delete(id);
        }
        this.toasts = this.toasts.filter((toast) => toast.id !== id);
    }

    /**
     * Cleans up all active timeouts and clears the toast state
     * Should be called when the component is destroyed to prevent memory leaks
     */
    cleanup() {
        for (const timeout of this.toastTimeoutMap.values()) {
            clearTimeout(timeout);
        }
        this.toastTimeoutMap.clear();
        this.toasts = [];
    }
}

/** Global instance of ToastState for direct usage */
export const toastState = new ToastState();

const TOAST_KEY = Symbol("WhitenoiseToastKey");

/**
 * Sets up the toast context for the current component tree
 */
export function setToastState() {
    return setContext(TOAST_KEY, new ToastState());
}

/**
 * Retrieves the toast state from the current context
 */
export function getToastState() {
    return getContext<ReturnType<typeof setToastState>>(TOAST_KEY);
}
