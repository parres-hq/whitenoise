# KeyboardAvoidingView

A component that automatically adjusts content when the mobile keyboard appears, ensuring form elements and buttons remain visible on screen.

## Features

- Detects keyboard visibility using the Visual Viewport API
- Adjusts content with smooth animations when keyboard appears/disappears
- Supports different adjustment strategies (padding or position)
- Works well with fixed/sticky elements at the bottom of the screen
- Special handling for sheet/modal components
- Cross-platform (iOS and Android)

## Usage

### Basic Usage

Wrap your content with the KeyboardAvoidingView component:

```svelte
<script>
  import KeyboardAvoidingView from "$lib/components/keyboard-avoiding-view";
</script>

<KeyboardAvoidingView>
  <div class="form-container">
    <input type="text" placeholder="Enter some text" />
    <button>Submit</button>
  </div>
</KeyboardAvoidingView>
```

### With Bottom Sheet Components

For bottom sheets or modals, use the `withSheet` prop:

```svelte
<Sheet.Content side="bottom">
  <KeyboardAvoidingView withSheet={true}>
    <div class="sheet-content">
      <input type="text" placeholder="Enter some text" />
    </div>
    <div class="fixed-bottom-actions">
      <button>Submit</button>
    </div>
  </KeyboardAvoidingView>
</Sheet.Content>
```

### With Fixed Bottom Elements

For pages with fixed bottom elements:

```svelte
<KeyboardAvoidingView>
  <div class="scrollable-content">
    <!-- Your scrollable content here -->
  </div>
  <div class="fixed bottom-0 left-0 right-0">
    <button class="w-full">Save</button>
  </div>
</KeyboardAvoidingView>
```

## Props

| Prop           | Type                      | Default     | Description                                    |
| -------------- | ------------------------- | ----------- | ---------------------------------------------- |
| `class`        | `string`                  | `""`        | Additional CSS classes to add to the container |
| `withSheet`    | `boolean`                 | `false`     | Set to true if used with a sheet component     |
| `bottomOffset` | `number`                  | `0`         | Additional space to add below the content      |
| `strategy`     | `"padding" \| "position"` | `"padding"` | How to handle keyboard appearance              |

## How It Works

The component uses the Visual Viewport API to detect when the keyboard appears and measure its height. When the keyboard is visible, the component either:

1. Adds padding to push content up (with `strategy="padding"`)
2. Adjusts the bottom position (with `strategy="position"`)

This ensures that input fields and action buttons remain visible when the user is typing.

## Browser Compatibility

The component requires the Visual Viewport API, which is supported in all modern browsers. For older browsers, the component will gracefully degrade (content won't adjust for keyboard).
