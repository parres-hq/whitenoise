<script lang="ts">
export let size = 80;
export let fullscreen = true;
</script>

<div class={fullscreen ? "w-screen h-svh" : "flex flex-col items-center justify-center"}>
    <div class="lds-ripple" style="--size:{size}px;">
        <div></div>
        <div></div>
    </div>
</div>

<style>
    .lds-ripple,
    .lds-ripple div {
        box-sizing: border-box;
    }
    .lds-ripple {
        display: inline-block;
        position: relative;
        --size: 80px; /* Default size */
        width: var(--size);
        height: var(--size);
    }
    .lds-ripple div {
        position: absolute;
        border: calc(var(--size) * 0.08) solid currentColor;
        opacity: 1;
        border-radius: 50%;
        animation: lds-ripple 1.5s cubic-bezier(0, 0.2, 0.8, 1) infinite;
    }
    .lds-ripple div:nth-child(2) {
        animation-delay: -0.5s;
    }
    @keyframes lds-ripple {
        0% {
            top: calc(var(--size) * 0.45);
            left: calc(var(--size) * 0.45);
            width: calc(var(--size) * 0.1);
            height: calc(var(--size) * 0.1);
            opacity: 0;
        }
        4.9% {
            top: calc(var(--size) * 0.45);
            left: calc(var(--size) * 0.45);
            width: calc(var(--size) * 0.1);
            height: calc(var(--size) * 0.1);
            opacity: 0;
        }
        5% {
            top: calc(var(--size) * 0.45);
            left: calc(var(--size) * 0.45);
            width: calc(var(--size) * 0.1);
            height: calc(var(--size) * 0.1);
            opacity: 1;
        }
        100% {
            top: 0;
            left: 0;
            width: var(--size);
            height: var(--size);
            opacity: 0;
        }
    }
</style>
