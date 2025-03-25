<script lang="ts" module>
import { defineMeta } from "@storybook/addon-svelte-csf";
import OnboardingNumbers from "./OnboardingNumbers.svelte";

const { Story } = defineMeta({
    title: "Components/OnboardingNumbers",
    component: OnboardingNumbers,
    tags: ["autodocs"],
    parameters: {
        docs: {
            story: {
                inline: true,
            },
        },
    },
});
</script>

<Story name="Step 1">
    <div class="bg-gray-900 p-4">
        <OnboardingNumbers 
            currentStep={1}
            inboxRelaysPublished={false}
            keyPackageRelaysPublished={false}
            keyPackagePublished={false}
        />
    </div>
</Story>

<Story name="Step 2">
    <div class="bg-gray-900 p-4">
        <OnboardingNumbers 
            currentStep={2}
            inboxRelaysPublished={true}
            keyPackageRelaysPublished={false}
            keyPackagePublished={false}
        />
    </div>
</Story>

<Story name="Step 3">
    <div class="bg-gray-900 p-4">
        <OnboardingNumbers 
            currentStep={3}
            inboxRelaysPublished={true}
            keyPackageRelaysPublished={true}
            keyPackagePublished={false}
        />
    </div>
</Story>

<Story name="All Complete">
    <div class="bg-gray-900 p-4">
        <OnboardingNumbers 
            currentStep={3}
            inboxRelaysPublished={true}
            keyPackageRelaysPublished={true}
            keyPackagePublished={true}
        />
    </div>
</Story> 
