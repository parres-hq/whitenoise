import type { LightningInvoice, LightningPayment } from "$lib/types/chat";
import type { NEvent } from "$lib/types/nostr";
import { toDataURL } from "qrcode";
import { findBolt11Tag, findPreimage } from "./tags";

export function eventToLightningInvoice(event: NEvent): LightningInvoice | undefined {
    const bolt11Tag = findBolt11Tag(event);
    if (!bolt11Tag?.[1]) return;
    const invoice = bolt11Tag[1];
    const amount = Number(bolt11Tag[2] || 0) / 1000;
    const description = bolt11Tag[3];
    const lightningInvoice: LightningInvoice = { invoice, amount, description, isPaid: false };
    return lightningInvoice;
}

export function eventToLightningPayment(event: NEvent): LightningPayment | undefined {
    const preimage = findPreimage(event);
    if (!preimage) return;
    const isPaid = event.tags.some((t) => t[0] === "q" && t[1] === event.id);
    return { preimage, isPaid };
}

export async function lightningInvoiceToQRCode(invoice: string): Promise<string> {
    try {
        return await toDataURL(`lightning:${invoice}`);
    } catch (error) {
        console.error("Error generating QR code:", error);
        return "";
    }
}
