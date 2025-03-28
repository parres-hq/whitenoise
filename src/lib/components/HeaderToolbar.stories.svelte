<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import { CaretLeft, PlusCircle } from "phosphor-svelte";
import HeaderToolbar from "./HeaderToolbar.svelte";

const { Story } = defineMeta({
    title: "Components/HeaderToolbar",
    component: HeaderToolbar,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
    argTypes: {
        left: {
            control: false,
            description: "Left content",
        },
        center: {
            control: false,
            description: "Center content",
        },
        right: {
            control: false,
            description: "Right content",
        },
        alwaysShowCenter: {
            control: false,
            description: "Whether to always show the center content",
        },
        bgColor: {
            control: "text",
            description: "Background color class for the toolbar",
        },
    },
});
</script>

<Story name="All Snippets">
  <HeaderToolbar alwaysShowCenter={true}>
    {#snippet left()}
      <button class="flex flex-row gap-0.5 items-end">
        <CaretLeft size={24} />
        <span class="text-xl font-medium">Back</span>
      </button>
    {/snippet}
    {#snippet center()}
      <h1>Modal Title</h1>
    {/snippet}
    {#snippet right()}
      <div>
        <button class="p-2 -mr-2">
          <PlusCircle size={30} />
        </button>
      </div>
    {/snippet}
  </HeaderToolbar>
</Story>
<Story name="alwaysShowCenter=false">
  <HeaderToolbar alwaysShowCenter={false}>
    {#snippet left()}
      <button class="flex flex-row gap-0.5 items-end">
        <CaretLeft size={24} />
        <span class="text-xl font-medium">Back</span>
      </button>
    {/snippet}
    {#snippet center()}
      <h1>Modal Title</h1>
    {/snippet}
    {#snippet right()}
      <div>
        <button class="p-2 -mr-2">
          <PlusCircle size={30} />
        </button>
      </div>
    {/snippet}
  </HeaderToolbar>
</Story>

<Story name="Left Snippet">
  <HeaderToolbar alwaysShowCenter={true}>
    {#snippet left()}
      <button class="flex flex-row gap-0.5 items-end">
        <CaretLeft size={24} />
        <span class="text-xl font-medium">Back</span>
      </button>
    {/snippet}
  </HeaderToolbar>
</Story>

<Story name="Right Snippet">
  <HeaderToolbar alwaysShowCenter={true}>
    {#snippet right()}
      <div>
        <button class="p-2 -mr-2">
          <PlusCircle size={30} />
        </button>
      </div>
    {/snippet}
  </HeaderToolbar>
</Story>

<Story name="Center Snippet">
  <HeaderToolbar alwaysShowCenter={true}>
    {#snippet center()}
      <h1>Modal Title</h1>
    {/snippet}
  </HeaderToolbar>
</Story> 
