# White Noise

A secure, private, and decentralized chat app built on Nostr, using the MLS protocol under the hood.

## Overview

White Noise aims to be the most secure private chat app on Nostr, with a focus on privacy and security. Under the hood, it uses the [Messaging Layer Security](https://www.rfc-editor.org/rfc/rfc9420.html) (MLS) protocol to manage group communications in a highly secure way. Nostr is used as the transport protocol and as the framework for the ongoing conversation in each chat.

## Status

![CI](https://github.com/erskingardner/whitenoise/actions/workflows/ci.yml/badge.svg?event=push)

![Linux Build](https://github.com/erskingardner/whitenoise/actions/workflows/build_linux.yml/badge.svg?event=push) ![Android Build](https://github.com/erskingardner/whitenoise/actions/workflows/build_android.yml/badge.svg?event=push)

![MacOS Build](https://github.com/erskingardner/whitenoise/actions/workflows/build_macos.yml/badge.svg?event=push) ![iOS Build](https://github.com/erskingardner/whitenoise/actions/workflows/build_ios.yml/badge.svg?event=push)

![Release](https://github.com/erskingardner/whitenoise/actions/workflows/release.yml/badge.svg?event=push)

## The Spec

White Noise is an implemenetation of the [NIP-104](https://github.com/nostr-protocol/nips/pull/1427) spec. This is still a draft spec, so it may change before it is finalized.

## Running White Noise

To build the app in release mode for desktop, run `bun tauri build`.

### MacOS

- On MacOS, running `bun tauri build` will create both a `.app` bundle and a `.dmg` installer. Install the `.dmg` in the same way you would install any other MacOS application.
- To update the app, run `git pull` and `bun tauri build` again. The app very much alpha so expect that updates will break your groups and chats. If you end up in an unrecoverable state, you can try deleting the `~/Library/Application Support/chat.whitenoise.dev` directory and running the app again. When you delete that directory, you're deleting all the MLS group state so you'll need to re-create or re-join the groups you had before and your previous history will be lost.

### Linux

- To provide a stable and reproducible build environment for Linux, the provided Dockerfile can be used:

```
docker build -t tauri-app -f Dockerfile.linux_build .
docker run --rm -v $(pwd):/app tauri-app
```

The resulting build artifacts (`.deb`, `.rpm` and `.AppImage` packages) will be created in the `src-tauri/target/release/bundle` directory.

### Windows

- I haven't tried the app on Windows yet. Let me know how it goes!

### Android

- To build the app on Android, it's easiest to run `bun tauri android build --apk --split-per-abi`. This will generate a number of APKs, one for each architecture. You can then install the one that matches your phone.

### iOS

- Right now, you can't build for iOS.

## Contributing

White Noise is built with [Tauri](https://tauri.app/) & [SvelteKit](https://kit.svelte.dev/). To get started contributing you'll need to have the [Rust](https://www.rust-lang.org/tools/install) toolchain installed and the [Bun](https://bun.sh/docs/installation) JavaScript package manager.

1. First off, you'll need to install [`nostr-rs-relay`](https://github.com/scsibug/nostr-rs-relay?tab=readme-ov-file) and run it locally when developing. At the moment, to simplify development everything happens over a local relay on `ws://localhost:8080`.
2. Clone the repo: `git clone https://github.com/erskingardner/whitenoise.git` and `cd whitenoise`.
3. Run `bun install` to install the front-end dependencies.
4. Run `bun tauri dev` to start the app. If you want to see more comprehensive logging, run `RUST_LOG=debug bun tauri dev`.

### Formatting and Linting

Before submitting code, please run both Clippy and Rustfmt to check for issues:

Clippy (linting):

```sh
cd src-tauri && cargo clippy --no-deps
```

Rustfmt

```sh
cd src-tauri && cargo fmt
```

## License

White Noise is free and open source software, released under the Gnu AGPL v3 license. See the [LICENSE](LICENSE) file for details.
