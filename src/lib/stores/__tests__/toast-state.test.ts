import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type Toast, ToastState } from "../toast-state.svelte";

describe("ToastState", () => {
    let toastState: ToastState;

    beforeEach(() => {
        vi.useFakeTimers();
        toastState = new ToastState();
    });

    afterEach(() => {
        toastState.cleanup();
        vi.restoreAllMocks();
    });

    describe("add", () => {
        it("should add a toast to the state", () => {
            toastState.add("Test Title", "Test Message", "info");

            expect(toastState.toasts).toHaveLength(1);
            expect(toastState.toasts[0]).toMatchObject({
                title: "Test Title",
                message: "Test Message",
                type: "info",
            });
        });

        it("should auto-remove toast after duration", () => {
            toastState.add("Test Title", "Test Message", "info", 5000);

            expect(toastState.toasts).toHaveLength(1);

            vi.advanceTimersByTime(5000);

            expect(toastState.toasts).toHaveLength(0);
        });
    });

    describe("remove", () => {
        it("should remove a toast by id", () => {
            toastState.add("Test Title", "Test Message", "info");
            const toastId = toastState.toasts[0].id;

            toastState.remove(toastId);

            expect(toastState.toasts).toHaveLength(0);
        });

        it("should clear timeout when removing toast", () => {
            const clearTimeoutSpy = vi.spyOn(global, "clearTimeout");
            toastState.add("Test Title", "Test Message", "info");
            const toastId = toastState.toasts[0].id;

            toastState.remove(toastId);

            expect(clearTimeoutSpy).toHaveBeenCalled();
        });
    });

    describe("cleanup", () => {
        it("should clear all timeouts and toasts", () => {
            const clearTimeoutSpy = vi.spyOn(global, "clearTimeout");
            toastState.add("Test Title 1", "Test Message 1", "info");
            toastState.add("Test Title 2", "Test Message 2", "error");

            toastState.cleanup();

            expect(clearTimeoutSpy).toHaveBeenCalledTimes(2);
            expect(toastState.toasts).toHaveLength(0);
        });
    });

    describe("toast types", () => {
        it("should handle all toast types", () => {
            const types: Toast["type"][] = ["info", "success", "error"];

            for (const type of types) {
                toastState.add("Test Title", "Test Message", type);
                expect(toastState.toasts[toastState.toasts.length - 1].type).toBe(type);
            }
        });
    });
});
