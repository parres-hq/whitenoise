# White Noise

A secure, private, and decentralized chat app built on Nostr, using the MLS protocol under the hood.

## Overview

White Noise aims to be the most secure private chat app on Nostr, with a focus on privacy and security. Under the hood, it uses the [Messaging Layer Security](https://www.rfc-editor.org/rfc/rfc9420.html) (MLS) protocol to manage group communications in a highly secure way. Nostr is used as the transport protocol and as the framework for the ongoing conversation in each chat.

This crate is the core library that powers our Flutter app. It is front-end agnostic and will allow for CLI and other interfaces to operate groups in the future.

## Status

![CI](https://github.com/parres-hq/whitenoise/actions/workflows/ci.yml/badge.svg?event=push)

## The Spec

White Noise is an implementation of the [NIP-EE](https://github.com/nostr-protocol/nips/pull/1427) spec.

## Releases

Coming soon
<!-- For the easist, quickest way to get started using White Noise, check out [the releases page](https://github.com/parres-hq/whitenoise/releases). -->

## Building White Noise Yourself

White Noise is a standard rust crate. Check it out and use it like you would any other crate.

1. Clone the repo: `git clone https://github.com/parres-hq/whitenoise.git` and `cd whitenoise`.

In addition, there is a `test-backend` binary in the codebase that runs through much of the API and functions as a sort of integration test. Run it with the following

```sh
just int-test
```

Check formatting, clippy, and docs with the following:

```
just check
```

## Contributing

To get started contributing you'll need to have the [Rust](https://www.rust-lang.org/tools/install) toolchain installed and [Docker](https://www.docker.com).

1. Clone the repo: `git clone https://github.com/parres-hq/whitenoise.git` and `cd whitenoise`.
1. In one terminal start the development services (two Nostr relays; nostr-rs-relay and strfry, and a blossom server) by running `docker compose up`.
1. Now you can run the integration test if you'd like.

### Formatting, Linting, and Tests

Before submitting PRs, please run the commad to check formatting and linting and run all tests:

```sh
just check
```

## License

White Noise is free and open source software, released under the Gnu AGPL v3 license. See the [LICENSE](LICENSE) file for details.
