<script lang="ts">
import { generateWhiteNoiseAvatar } from "$lib/utils/avatar";

// Sample pubkeys and group names to test avatar generation
const samples = [
    { type: "User", id: "04f73c2c308f48981299f068f939574af7541dd0fa9a9be70d93ac17a4144d561" },
    { type: "User", id: "3492d992dd225f46d881d0a7cb66a77315ef2c8cd75f9f9bf95e5e96c9cb62fd" },
    { type: "User", id: "7f0f0ff5df3e160b1e693aa14f570f9c79b4d6af30b8c3a6f28e56e172a0f886" },
    { type: "Group", id: "Developers" },
    { type: "Group", id: "Project Alpha" },
    { type: "Group", id: "Marketing Team" },
];

// Additional test samples with similar names to demonstrate uniqueness
const similarSamples = [
    { type: "User", id: "Alice" },
    { type: "User", id: "Alice1" },
    { type: "User", id: "Alice2" },
    { type: "Group", id: "Team A" },
    { type: "Group", id: "Team B" },
    { type: "Group", id: "Team C" },
];

// Deliberate test cases for TV effects
const tvTestCases = [
    { name: "Heavy Bands", id: "HeavyBanding123456789" },
    { name: "RGB Fringing", id: "RGBFringing987654321" },
    { name: "Warpy", id: "SuperWarpyEffect135792468" },
    { name: "Glitchy", id: "GlitchCity246813579" },
    { name: "Retro VHS", id: "RetroVHSLook9753124680" },
    { name: "Interference", id: "SignalInterference777888999" },
];

// Generate different sizes to test
const sizes = [32, 64, 96, 128];

// Generate different densities and colorfulness
const densities = [1, 2, 3, 4];
const colorfulness = [0.3, 0.6, 0.9];

// TV effect intensities
const tvEffects = [0, 0.25, 0.5, 0.8, 1.0];

// Generate a sample avatar
function generateSample(id: string, size: number, density = 2, colorful = 0.7, tvEffect = 0.8) {
    return generateWhiteNoiseAvatar(id, size, colorful, density, tvEffect);
}
</script>

<div class="p-4">
    <h2 class="text-xl font-bold mb-4">80s TV Static Avatar Generator</h2>
    <p class="text-sm text-gray-600 mb-6">
        Unique, deterministic avatars inspired by old TV static with RGB color separation, warped scanlines, and other effects that vary based on the input string.
    </p>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">Different IDs (Unique TV Effects per ID)</h3>
        <div class="flex flex-wrap gap-4">
            {#each samples as sample}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(sample.id, 64)}
                        alt="{sample.type} avatar"
                        class="rounded-full border border-gray-200"
                        width="64"
                        height="64"
                    />
                    <span class="text-xs text-gray-600">{sample.type}</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">Similar Names (Notice the Different TV Effects)</h3>
        <div class="flex flex-wrap gap-4">
            {#each similarSamples as sample}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(sample.id, 64)}
                        alt="{sample.type} avatar"
                        class="rounded-full border border-gray-200"
                        width="64"
                        height="64"
                    />
                    <span class="text-xs text-gray-600">{sample.id}</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">Different Sizes (Same ID)</h3>
        <div class="flex flex-wrap gap-4 items-end">
            {#each sizes as size}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(samples[0].id, size)}
                        alt="Size {size} avatar"
                        class="rounded-full border border-gray-200"
                        width={size}
                        height={size}
                    />
                    <span class="text-xs text-gray-600">{size}px</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">Different Densities</h3>
        <div class="flex flex-wrap gap-4">
            {#each densities as density}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(samples[1].id, 64, density)}
                        alt="Density {density} avatar"
                        class="rounded-full border border-gray-200"
                        width="64"
                        height="64"
                    />
                    <span class="text-xs text-gray-600">Density: {density}</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">Different Colorfulness</h3>
        <div class="flex flex-wrap gap-4">
            {#each colorfulness as color}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(samples[2].id, 64, 2, color)}
                        alt="Colorful {color} avatar"
                        class="rounded-full border border-gray-200"
                        width="64"
                        height="64"
                    />
                    <span class="text-xs text-gray-600">Colorfulness: {color}</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">TV Static Effect Intensity</h3>
        <div class="flex flex-wrap gap-4">
            {#each tvEffects as effect}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(samples[3].id, 64, 2, 0.7, effect)}
                        alt="TV Effect {effect} avatar"
                        class="rounded-full border border-gray-200"
                        width="64"
                        height="64"
                    />
                    <span class="text-xs text-gray-600">Intensity: {effect}</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mb-8">
        <h3 class="text-lg font-semibold mb-2">TV Effect Showcase (Larger Size)</h3>
        <div class="flex flex-wrap gap-6">
            {#each tvTestCases as testCase}
                <div class="flex flex-col items-center gap-2">
                    <img
                        src={generateSample(testCase.id, 128, 2, 0.7, 1.0)}
                        alt="{testCase.name} avatar"
                        class="rounded-full border border-gray-200"
                        width="128"
                        height="128"
                    />
                    <span class="text-xs text-gray-600">{testCase.name}</span>
                </div>
            {/each}
        </div>
    </div>

    <div class="mt-8">
        <h3 class="text-lg font-semibold mb-2">Maximum TV Effects</h3>
        <div class="flex justify-center">
            <div class="flex flex-col items-center gap-2">
                <img
                    src={generateSample("UltimateRetroTVAvatar", 200, 3, 0.8, 1.0)}
                    alt="Ultimate TV avatar"
                    class="rounded-full border border-gray-200"
                    width="200"
                    height="200"
                />
                <span class="text-xs text-gray-600">200px, Full TV Static Effect</span>
            </div>
        </div>
    </div>
</div>
