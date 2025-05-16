import { decode } from "blurhash";

export function blurhashToSVG(blurhash: string, width = 64, height = 64): string {
    const pixels = decode(blurhash, width, height);
    let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`;
    for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
            const idx = 4 * (y * width + x);
            const r = pixels[idx];
            const g = pixels[idx + 1];
            const b = pixels[idx + 2];
            svg += `<rect x="${x}" y="${y}" width="1" height="1" fill="rgb(${r},${g},${b})" shape-rendering="crispEdges"/>`;
        }
    }
    svg += "</svg>";
    return `data:image/svg+xml;base64,${btoa(svg)}`;
}
