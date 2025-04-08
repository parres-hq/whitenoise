/**
 * Utility for generating white noise inspired avatars
 */

/**
 * Generate a white noise inspired avatar as a data URL
 * This uses a deterministic algorithm based on the input string
 *
 * @param inputString - Unique identifier (pubkey, group ID, etc.)
 * @param size - Canvas size in pixels (default: 128)
 * @param colorfulness - How colorful the noise should be (0-1, default: 0.7)
 * @param density - Density of the noise pattern (0.5-5, default: 2)
 * @param tvEffect - How strong the TV distortion effect should be (0-1, default: 0.8)
 * @returns Data URL of the generated avatar
 */
export function generateWhiteNoiseAvatar(
    inputString: string,
    size = 128,
    colorfulness = 0.7,
    density = 2,
    tvEffect = 0.8
): string {
    // Create canvas
    const canvas = document.createElement("canvas");
    canvas.width = size;
    canvas.height = size;
    const ctx = canvas.getContext("2d");

    if (!ctx) {
        console.error("Canvas context not available");
        return "";
    }

    // Create a seed from the input string
    const seed = stringToSeed(inputString);

    // Create a pseudo-random number generator
    const rng = mulberry32(seed);

    // Generate a more distinctive color palette based on the input string
    const colorParams = deriveColorParams(inputString);

    // Derive TV effect parameters from the input string for unique styling per avatar
    const tvEffectParams = deriveTvEffectParams(inputString, tvEffect, rng);

    // Generate the noise pattern
    const imageData = ctx.createImageData(size, size);
    const data = imageData.data;

    // For each pixel
    for (let i = 0; i < size; i++) {
        for (let j = 0; j < size; j++) {
            // Apply TV warping effect - distort the sampling coordinates
            let sampleX = j;
            let sampleY = i;

            if (tvEffect > 0) {
                // Horizontal warp (like TV with bad h-sync)
                const timeOffset = seed % 1000;
                const waveHeight = size * 0.05 * tvEffect * tvEffectParams.warpIntensity;
                const waveFreq = tvEffectParams.warpFrequency;

                // Create horizontal wavy distortion
                sampleY += Math.sin((j / size) * waveFreq * Math.PI + timeOffset) * waveHeight;

                // Create vertical rolling effect
                const vertShift =
                    Math.sin(seed / 1000) *
                    size *
                    0.08 *
                    tvEffect *
                    tvEffectParams.verticalRollIntensity;
                sampleY = (sampleY + vertShift) % size;
                if (sampleY < 0) sampleY += size;

                // Random horizontal glitches
                if (rng() < 0.03 * tvEffect * tvEffectParams.glitchProbability) {
                    sampleX +=
                        (rng() - 0.5) * size * 0.1 * tvEffect * tvEffectParams.glitchIntensity;
                }
            }

            // Keep coordinates in bounds
            sampleX = Math.max(0, Math.min(size - 1, sampleX));
            sampleY = Math.max(0, Math.min(size - 1, sampleY));

            const idx = (i * size + j) * 4;

            // Create deterministic noise value
            const noiseValue = simplex2D(
                (sampleX * density) / size,
                (sampleY * density) / size,
                seed
            );

            // Determine if we should use a colorful pixel or grayscale
            const useColor = rng() < colorfulness;

            if (useColor) {
                // Use our enhanced color generation based on the input string
                const { r, g, b } = getColorForNoise(noiseValue, colorParams, rng);

                data[idx] = r; // R
                data[idx + 1] = g; // G
                data[idx + 2] = b; // B
            } else {
                // Grayscale pixel with subtle color tinting from the palette
                const value = Math.floor(128 + noiseValue * 128);
                // Apply subtle color tint even to grayscale pixels
                const tintAmount = 0.15; // How much tint to apply

                data[idx] = Math.floor(
                    value * (1 - tintAmount) + value * colorParams.grayTintR * tintAmount
                ); // R with tint
                data[idx + 1] = Math.floor(
                    value * (1 - tintAmount) + value * colorParams.grayTintG * tintAmount
                ); // G with tint
                data[idx + 2] = Math.floor(
                    value * (1 - tintAmount) + value * colorParams.grayTintB * tintAmount
                ); // B with tint
            }

            // Apply TV scanline effect
            if (tvEffect > 0) {
                // Create horizontal scanlines with variable width and spacing
                const scanlineFrequency = size / tvEffectParams.scanlineFrequency; // Use derived frequency
                const scanlineVariance = tvEffectParams.scanlineVariance * tvEffect; // Use derived variance

                // Determine if this should be a bent scanline - increased probability
                const isBentLine =
                    rng() < tvEffectParams.bentLineProb * tvEffect &&
                    i % Math.floor(tvEffectParams.bentLineSpacing) === 0;
                let scanlineFactor = 1;

                // Track if we're near a scanline for RGB shifting
                let isNearScanline = false;
                let scanlineDistance = 0;

                if (isBentLine) {
                    // Create a bent/wavy scanline with stronger effect
                    const bendAmount =
                        tvEffectParams.minBend + rng() * tvEffectParams.bendRangeWidth * tvEffect;
                    const bendFreq = tvEffectParams.bendFrequency;
                    const lineThickness =
                        tvEffectParams.minThickness +
                        Math.floor(rng() * tvEffectParams.thicknessRange);

                    // Check if we're within the bent line's thickness
                    const bendLine =
                        Math.sin((j / size) * bendFreq * Math.PI * 2) * size * bendAmount;
                    const distFromBendLine = Math.abs(i - (i + bendLine));

                    if (distFromBendLine < lineThickness) {
                        scanlineFactor =
                            tvEffectParams.minDarkness + rng() * tvEffectParams.darknessRange;
                        isNearScanline = true;
                        scanlineDistance = distFromBendLine / lineThickness;
                    } else if (distFromBendLine < lineThickness * 3) {
                        // Near a scanline - for RGB shift effects
                        isNearScanline = true;
                        scanlineDistance = distFromBendLine / (lineThickness * 3);
                    }
                } else {
                    // Regular scanlines with variance - stronger effect
                    const baseScanline = (i * (Math.PI * 2)) / scanlineFrequency;
                    // More horizontal variance for wavy scanlines
                    const variance = (Math.sin(j * 0.2) + Math.cos(j * 0.13)) * scanlineVariance;
                    const scanlineValue = Math.sin(baseScanline + variance);
                    const scanlineEffect =
                        scanlineValue * tvEffectParams.scanlineIntensity * tvEffect;

                    // Make some scanlines much more prominent
                    const isProminentLine =
                        i % Math.floor(tvEffectParams.prominentLineSpacing) === 0;
                    const scanlineIntensity = isProminentLine
                        ? tvEffectParams.prominentLineDarkness * tvEffect // More pronounced line
                        : scanlineEffect; // Normal line

                    scanlineFactor = 1 - scanlineIntensity;

                    // Check if we're near a scanline for RGB shifting
                    const absValue = Math.abs(scanlineValue);
                    if (absValue > 0.7) {
                        isNearScanline = true;
                        scanlineDistance = (1 - absValue) * 3;
                    }

                    // Add occasional very dark bands to simulate complete signal loss
                    if (isProminentLine && rng() < tvEffectParams.blackoutProb * tvEffect) {
                        scanlineFactor = tvEffectParams.blackoutDarkness + rng() * 0.1; // Almost black line
                        isNearScanline = true;
                        scanlineDistance = 0;
                    }
                }

                // Store the original RGB values before applying scanline effect
                const originalR = data[idx];
                const originalG = data[idx + 1];
                const originalB = data[idx + 2];

                // Apply the calculated scanline effect
                data[idx] = Math.max(0, Math.floor(originalR * scanlineFactor));
                data[idx + 1] = Math.max(0, Math.floor(originalG * scanlineFactor));
                data[idx + 2] = Math.max(0, Math.floor(originalB * scanlineFactor));

                // Apply RGB color shifting/chromatic aberration near scanlines
                if (isNearScanline && tvEffect > 0.3) {
                    // Calculate color shift amount based on proximity to scanline
                    const shiftIntensity =
                        (1 - scanlineDistance) * tvEffect * tvEffectParams.rgbShiftIntensity;

                    // Direction of the shift (positive or negative)
                    const shiftDirection = j % 2 === 0 ? 1 : -1;

                    // Calculate RGB shift distances - different for each channel
                    const rShift =
                        Math.floor(shiftIntensity * size * tvEffectParams.redShiftAmount) *
                        shiftDirection;
                    const gShift = 0; // Keep green channel at original position
                    const bShift =
                        Math.floor(shiftIntensity * size * tvEffectParams.blueShiftAmount) *
                        -shiftDirection;

                    // Apply shifts to create chromatic aberration
                    // Red channel shifted
                    if (j + rShift >= 0 && j + rShift < size) {
                        const rShiftIdx = (i * size + (j + rShift)) * 4;
                        if (rShiftIdx >= 0 && rShiftIdx < data.length) {
                            data[rShiftIdx] = Math.max(
                                data[rShiftIdx],
                                Math.floor(originalR * 0.8)
                            );
                        }
                    }

                    // Blue channel shifted in opposite direction
                    if (j + bShift >= 0 && j + bShift < size) {
                        const bShiftIdx = (i * size + (j + bShift)) * 4;
                        if (bShiftIdx >= 0 && bShiftIdx < data.length) {
                            data[bShiftIdx + 2] = Math.max(
                                data[bShiftIdx + 2],
                                Math.floor(originalB * 0.8)
                            );
                        }
                    }
                }

                // Random color bleeding/artifacts (RGB color shift) - increased probability and intensity
                if (rng() < tvEffectParams.colorBleedProb * tvEffect) {
                    const shiftAmount = Math.floor(
                        rng() * tvEffectParams.colorBleedAmount * tvEffect
                    );
                    const channelToShift = Math.floor(rng() * 3);

                    if (channelToShift === 0) {
                        data[idx] = Math.min(255, data[idx] + shiftAmount);
                    } else if (channelToShift === 1) {
                        data[idx + 1] = Math.min(255, data[idx + 1] + shiftAmount);
                    } else {
                        data[idx + 2] = Math.min(255, data[idx + 2] + shiftAmount);
                    }
                }

                // Add occasional horizontal color shift/ghosting effect
                if (
                    rng() < tvEffectParams.ghostingProb * tvEffect &&
                    j > size * 0.05 &&
                    j < size * 0.95
                ) {
                    const shiftDistance = Math.floor(
                        size * tvEffectParams.ghostingAmount * tvEffect
                    );
                    const ghostIdx = (i * size + (j - shiftDistance)) * 4;

                    if (ghostIdx >= 0 && ghostIdx < data.length - 3) {
                        data[idx] = Math.max(data[idx], Math.floor(data[ghostIdx] * 0.7));
                        data[idx + 1] = Math.max(
                            data[idx + 1],
                            Math.floor(data[ghostIdx + 1] * 0.7)
                        );
                        data[idx + 2] = Math.max(
                            data[idx + 2],
                            Math.floor(data[ghostIdx + 2] * 0.7)
                        );
                    }
                }
            }

            // Full opacity
            data[idx + 3] = 255; // A
        }
    }

    ctx.putImageData(imageData, 0, 0);
    return canvas.toDataURL("image/png");
}

/**
 * Derive TV effect parameters from the input string
 * This ensures each avatar has a unique TV effect style
 */
function deriveTvEffectParams(inputString: string, baseEffect: number, rng: () => number) {
    // Create a secondary seed for parameter derivation
    const paramSeed = stringToSeed(inputString.split("").reverse().join(""));
    const paramRng = mulberry32(paramSeed);

    // Create character-based variations
    const charSum = inputString.split("").reduce((sum, char) => sum + char.charCodeAt(0), 0);
    const stringLength = inputString.length;

    // Calculate normalized parameters (0-1 range)
    const norm1 = (charSum % 100) / 100; // 0-1 based on character sum
    const norm2 = (stringLength % 20) / 20; // 0-1 based on string length
    const norm3 = paramRng(); // First random based on param seed
    const norm4 = paramRng(); // Second random based on param seed

    return {
        // Parameters that affect warp and distortion
        warpIntensity: 0.5 + norm1 * 1.5, // 0.5-2.0
        warpFrequency: 1 + norm2 * 5, // 1-6
        verticalRollIntensity: 0.5 + norm3 * 1.5, // 0.5-2.0
        glitchProbability: 0.7 + norm4 * 0.6, // 0.7-1.3
        glitchIntensity: 0.8 + norm1 * 0.8, // 0.8-1.6

        // Parameters that affect scanline frequency and appearance
        scanlineFrequency: 4 + norm3 * 8, // 4-12
        scanlineVariance: 2 + norm4 * 3, // 2-5
        scanlineIntensity: 0.25 + norm2 * 0.2, // 0.25-0.45

        // Parameters for bent lines
        bentLineProb: 0.2 + norm1 * 0.2, // 0.2-0.4
        bentLineSpacing: 2 + Math.floor(norm2 * 5), // 2-7
        minBend: 0.15 + norm3 * 0.25, // 0.15-0.4
        bendRangeWidth: 0.3 + norm4 * 0.4, // 0.3-0.7
        bendFrequency: 1 + norm1 * 4, // 1-5

        // Parameters for line thickness
        minThickness: 1 + Math.floor(norm2 * 2), // 1-3
        thicknessRange: 2 + Math.floor(norm3 * 3), // 2-5

        // Parameters for line darkness
        minDarkness: 0.05 + norm4 * 0.15, // 0.05-0.2
        darknessRange: 0.15 + norm1 * 0.25, // 0.15-0.4

        // Parameters for prominent lines
        prominentLineSpacing: 2 + Math.floor(norm2 * 4), // 2-6
        prominentLineDarkness: 0.3 + norm3 * 0.4, // 0.3-0.7

        // Parameters for blackout bands
        blackoutProb: 0.1 + norm4 * 0.15, // 0.1-0.25
        blackoutDarkness: 0.02 + norm1 * 0.1, // 0.02-0.12

        // RGB shift parameters
        rgbShiftIntensity: 0.7 + norm2 * 0.6, // 0.7-1.3
        redShiftAmount: 0.02 + norm3 * 0.03, // 0.02-0.05
        blueShiftAmount: 0.03 + norm4 * 0.04, // 0.03-0.07

        // Color bleeding parameters
        colorBleedProb: 0.06 + norm1 * 0.07, // 0.06-0.13
        colorBleedAmount: 40 + norm2 * 30, // 40-70

        // Ghosting effect parameters
        ghostingProb: 0.005 + norm3 * 0.015, // 0.005-0.02
        ghostingAmount: 0.01 + norm4 * 0.03, // 0.01-0.04
    };
}

/**
 * Convert a string to a numeric seed
 */
function stringToSeed(str: string): number {
    let hash = 0;
    const prime1 = 31;
    const prime2 = 486187739;

    // Use a more distinctive hashing algorithm
    for (let i = 0; i < str.length; i++) {
        // Combine multiple approaches for better distribution
        const char = str.charCodeAt(i);
        hash = Math.imul(hash, prime1) + char;
        hash = hash ^ (hash >>> 9);
        hash = Math.imul(hash, prime2);
    }

    // Final mixing step
    hash = hash ^ (hash >>> 16);
    hash = Math.imul(hash, 2246822507);
    hash = hash ^ (hash >>> 13);
    hash = Math.imul(hash, 3266489909);
    hash = hash ^ (hash >>> 16);

    return Math.abs(hash);
}

/**
 * Simple PRNG based on the input seed
 */
function mulberry32(initialSeed: number): () => number {
    let seed = initialSeed;
    return () => {
        // Use a stronger algorithm that produces more varied outputs
        seed += 0x6d2b79f5;
        let t = seed;
        t = Math.imul(t ^ (t >>> 15), t | 1);
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
        t = ((t ^ (t >>> 14)) >>> 0) / 4294967296;

        // Additional randomization for better distribution
        return t < 0.5 ? t * t : 1 - (1 - t) * (1 - t);
    };
}

/**
 * Simplex-like 2D noise function (very fast approximation)
 * Deterministic based on input coordinates and seed
 */
function simplex2D(x: number, y: number, seed: number): number {
    // Adjust coordinates based on seed with more variation
    const seedOffset = seed % 10000;
    const seedX = x + (seedOffset % 1000) / 500 + Math.sin(seed * 0.01) * 0.5;
    const seedY = y + (seedOffset % 700) / 350 + Math.cos(seed * 0.01) * 0.5;

    // Apply more rotation based on the seed
    const angle = (seed % 628) / 100; // 0 to 2π range
    const rotX = seedX * Math.cos(angle) - seedY * Math.sin(angle);
    const rotY = seedX * Math.sin(angle) + seedY * Math.cos(angle);

    const px = Math.floor(rotX);
    const py = Math.floor(rotY);

    const dx = rotX - px;
    const dy = rotY - py;

    // More varied hash combinations
    const n1 = hash(px, py, seed) * 0.5 + 0.5;
    const n2 = hash(px + 1, py, seed) * 0.5 + 0.5;
    const n3 = hash(px, py + 1, seed) * 0.5 + 0.5;
    const n4 = hash(px + 1, py + 1, seed) * 0.5 + 0.5;

    // Add more variation to corner values based on seed
    const seedVariation = (seed % 100) / 100;
    const n1v = n1 * (1 + seedVariation * 0.2);
    const n2v = n2 * (1 - seedVariation * 0.1);
    const n3v = n3 * (1 + seedVariation * 0.15);
    const n4v = n4 * (1 - seedVariation * 0.25);

    // Improved interpolation for more varied patterns
    const ix1 = lerp(n1v, n2v, easeInOut(dx));
    const ix2 = lerp(n3v, n4v, easeInOut(dx));
    return lerp(ix1, ix2, easeInOut(dy));
}

/**
 * Fast hash function for noise generation with better distribution
 */
function hash(x: number, y: number, seed: number): number {
    // Use different primes for more varied hashing
    const prime1 = 73;
    const prime2 = 149;
    const prime3 = 631;

    // Combine multiple hash approaches for better distribution
    let h = (x * prime1 + y * prime2 + seed * prime3) & 0x7fffffff;
    h = (h << 13) ^ h;
    h = (h * (h * h * 15731 + 789221) + 1376312589) & 0x7fffffff;

    return 2 * (h / 0x7fffffff - 0.5);
}

/**
 * Improved non-linear interpolation
 */
function easeInOut(t: number): number {
    // Cubic easing for smoother transitions
    return t * t * (3 - 2 * t);
}

/**
 * Linear interpolation
 */
function lerp(a: number, b: number, t: number): number {
    return a + t * (b - a);
}

/**
 * Convert HSL color to RGB
 */
function hslToRgb(
    hue: number,
    saturation: number,
    lightness: number
): { r: number; g: number; b: number } {
    const h = hue / 360;
    let r: number;
    let g: number;
    let b: number;

    if (saturation === 0) {
        r = g = b = lightness;
    } else {
        const hue2rgb = (p: number, q: number, t: number): number => {
            let tValue = t;
            if (tValue < 0) tValue += 1;
            if (tValue > 1) tValue -= 1;
            if (tValue < 1 / 6) return p + (q - p) * 6 * tValue;
            if (tValue < 1 / 2) return q;
            if (tValue < 2 / 3) return p + (q - p) * (2 / 3 - tValue) * 6;
            return p;
        };

        const q =
            lightness < 0.5
                ? lightness * (1 + saturation)
                : lightness + saturation - lightness * saturation;
        const p = 2 * lightness - q;
        r = hue2rgb(p, q, h + 1 / 3);
        g = hue2rgb(p, q, h);
        b = hue2rgb(p, q, h - 1 / 3);
    }

    return {
        r: Math.round(r * 255),
        g: Math.round(g * 255),
        b: Math.round(b * 255),
    };
}

/**
 * Derive color parameters from the input string to ensure
 * each avatar has a distinctive color palette
 */
function deriveColorParams(inputString: string) {
    // Create a seed specifically for color generation
    const colorSeed = stringToSeed(`${inputString.split("").reverse().join("")}color`);
    const colorRng = mulberry32(colorSeed);

    // Character frequency analysis for more unique color derivation
    const charFreq: { [key: string]: number } = {};
    let totalChars = 0;

    for (const char of inputString) {
        charFreq[char] = (charFreq[char] || 0) + 1;
        totalChars++;
    }

    // Determine character diversity (0-1)
    const uniqueChars = Object.keys(charFreq).length;
    const diversity = Math.min(1, uniqueChars / Math.max(1, totalChars));

    // Generate a primary hue based on the string content with better distribution
    // Use a more complex algorithm to avoid bias toward certain hue ranges

    // Get a more uniform initial distribution of hues
    const byteSum = inputString.split("").reduce((sum, char) => sum + char.charCodeAt(0), 0);
    const charParity = inputString.length % 6; // 0-5 range for additional variation

    // Create 6 color regions (red, orange, yellow, green, blue, purple) with even distribution
    // Map the input string characteristics to one of these regions first
    const colorRegions = [
        { name: "red", hueRange: [0, 30] },
        { name: "orange", hueRange: [30, 60] },
        { name: "yellow", hueRange: [60, 90] },
        { name: "green", hueRange: [90, 180] },
        { name: "blue", hueRange: [180, 270] },
        { name: "purple", hueRange: [270, 360] },
    ];

    // Determine the base color region using a combination of factors
    // This ensures a more even distribution across the spectrum
    let regionIndex = ((byteSum % 6) + charParity) % 6;

    // Add a correction factor to counteract the purple/pink bias
    // If we land in the purple region, have a chance to redistribute
    if (regionIndex === 5) {
        // purple region
        // 70% chance to redistribute to another region if we hit purple
        if (colorRng() < 0.7) {
            // Redistribute with higher chance for yellow, green, and blue regions
            const redistWeights = [0.15, 0.2, 0.25, 0.25, 0.15, 0]; // zero weight for purple
            const redistVal = colorRng();
            let cumulative = 0;

            for (let i = 0; i < redistWeights.length; i++) {
                cumulative += redistWeights[i];
                if (redistVal < cumulative) {
                    regionIndex = i;
                    break;
                }
            }
        }
    }

    // Select the region's hue range
    const selectedRegion = colorRegions[regionIndex];
    const [minHue, maxHue] = selectedRegion.hueRange;

    // Generate a specific hue within the selected region
    const hueRangeSize = maxHue - minHue;
    // Use modulo inside the range to avoid bias within the region
    const hueOffset = (colorSeed % hueRangeSize) + minHue;

    // Now we have a more uniform primary hue
    const primaryHue = hueOffset;

    // Choose a color scheme type based on the input string with different weights
    // to ensure better distribution of schemes
    const schemeTypeRand = colorRng();
    let schemeType = 0;

    if (schemeTypeRand < 0.25) {
        schemeType = 0; // Analogous (25%)
    } else if (schemeTypeRand < 0.5) {
        schemeType = 1; // Complementary (25%)
    } else if (schemeTypeRand < 0.7) {
        schemeType = 2; // Triadic (20%)
    } else if (schemeTypeRand < 0.9) {
        schemeType = 3; // Tetradic (20%)
    } else {
        schemeType = 4; // Custom (10%)
    }

    // Primary colors of the palette
    let secondaryHue = 0;
    let tertiaryHue = 0;

    // Determine additional hues based on the scheme type
    switch (schemeType) {
        case 0: // Analogous
            secondaryHue = (primaryHue + 30) % 360;
            tertiaryHue = (primaryHue - 30 + 360) % 360;
            break;
        case 1: // Complementary
            secondaryHue = (primaryHue + 180) % 360;
            tertiaryHue = (primaryHue + 90) % 360;
            break;
        case 2: // Triadic
            secondaryHue = (primaryHue + 120) % 360;
            tertiaryHue = (primaryHue + 240) % 360;
            break;
        case 3: // Tetradic
            secondaryHue = (primaryHue + 90) % 360;
            tertiaryHue = (primaryHue + 180) % 360;
            break;
        case 4: // Custom/Random
            secondaryHue = (primaryHue + 60 + Math.floor(colorRng() * 240)) % 360;
            tertiaryHue = (primaryHue + 120 + Math.floor(colorRng() * 180)) % 360;
            break;
    }

    // Calculate saturation and lightness ranges based on the input string
    // Higher diversity = more saturated colors
    const baseSaturation = 0.7 + diversity * 0.3; // 0.7-1.0

    // Adjust lightness to improve visibility and vibrancy
    // Different color regions need different lightness to appear vibrant
    let baseLightness = 0.5 + colorRng() * 0.2; // 0.5-0.7 default

    // Yellow needs to be darker, blue needs to be lighter for optimal visibility
    if (regionIndex === 2) {
        // yellow
        baseLightness = Math.max(0.4, baseLightness - 0.1); // darker yellows
    } else if (regionIndex === 4) {
        // blue
        baseLightness = Math.min(0.7, baseLightness + 0.1); // lighter blues
    }

    // Calculate grayscale tint factors based on the primary color
    const { r, g, b } = hslToRgb(primaryHue, 0.2, 0.5);
    const maxRgb = Math.max(r, g, b) / 255;
    const grayTintR = r / 255 / Math.max(0.5, maxRgb);
    const grayTintG = g / 255 / Math.max(0.5, maxRgb);
    const grayTintB = b / 255 / Math.max(0.5, maxRgb);

    return {
        primaryHue,
        secondaryHue,
        tertiaryHue,
        schemeType,
        baseSaturation,
        baseLightness,
        colorVariability: 0.4 + colorRng() * 0.4, // 0.4-0.8
        noiseToHueScale: 50 + Math.floor(colorRng() * 50), // 50-100
        grayTintR,
        grayTintG,
        grayTintB,
        colorRegion: selectedRegion.name,
    };
}

/**
 * Get a color for a specific noise value based on the color parameters
 */
function getColorForNoise(
    noiseValue: number,
    colorParams: ReturnType<typeof deriveColorParams>,
    rng: () => number
): { r: number; g: number; b: number } {
    // Determine which color from our palette to use based on the noise value
    const paletteSelector = rng();

    // Adjust palette weights for better color distribution
    let baseHue: number;
    if (paletteSelector < 0.5) {
        // Primary color (50%)
        baseHue = colorParams.primaryHue;
    } else if (paletteSelector < 0.8) {
        // Secondary color (30%)
        baseHue = colorParams.secondaryHue;
    } else {
        // Tertiary color (20%)
        baseHue = colorParams.tertiaryHue;
    }

    // Add some variation based on the noise value
    const hueVariation = noiseValue * colorParams.noiseToHueScale;
    const hue = (baseHue + hueVariation) % 360;

    // Higher saturation for more vibrant colors, with slight randomness
    const saturationVariation = rng() * colorParams.colorVariability;
    const saturation = Math.min(1, colorParams.baseSaturation - saturationVariation * 0.3);

    // Variable lightness based on noise with a floor to prevent too-dark colors
    const lightnessVariation = Math.abs(noiseValue) * colorParams.colorVariability;
    const lightness = Math.min(0.9, Math.max(0.35, colorParams.baseLightness + lightnessVariation));

    return hslToRgb(hue, saturation, lightness);
}
