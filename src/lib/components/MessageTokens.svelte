<script lang="ts">
import type { SerializableToken } from "$lib/types/nostr";

export let tokens: SerializableToken[];
/**
 * When true, renders in reply mode: limits display to 2 lines with ellipsis and renders URLs as plain text
 */
export let reply = false;

function getTokenType(token: SerializableToken | string): string {
    if (typeof token === "string") {
        return token;
    }
    return Object.keys(token)[0];
}

function getTokenValue(token: SerializableToken | string): string | null {
    if (typeof token === "string") {
        return null;
    }
    const type = Object.keys(token)[0] as keyof SerializableToken;
    return (token as Record<string, string | null>)[type];
}
</script>

<div class="message-tokens" class:reply>
    {#each tokens as token}
        {#if 'Text' in token}
            <span class="text">{token.Text}</span>
        {:else if 'Url' in token}
          { #if reply}
            <span class="text">{token.Url}</span>
          {:else}
            <a href={token.Url} target="_blank" rel="noopener noreferrer" class="url">{token.Url}</a>
          {/if}
        {:else if 'Hashtag' in token}
            <span class="hashtag">#{token.Hashtag}</span>
        {:else if 'Nostr' in token}
            <span class="nostr">{token.Nostr}</span>
        {:else if 'LineBreak' in token}
            <br />
        {:else if 'Whitespace' in token}
            <span class="whitespace">&nbsp;</span>
        {/if}
    {/each}
</div>

<style>
    .message-tokens {
        display: inline;
        white-space: pre-wrap;
        word-break: break-word;
    }

    .message-tokens.reply {
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .url {
        color: #0066cc;
        text-decoration: underline;
    }

    .url:hover {
        color: #004499;
    }

    .hashtag {
        color: #1da1f2;
    }

    .nostr {
        color: #ff8c00;
    }

    .whitespace {
        display: inline;
    }
</style>
