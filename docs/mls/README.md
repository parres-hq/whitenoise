# MLS Protocol Documentation

This directory contains documentation and references for the MLS (Messaging Layer Security) protocol implementation in Whitenoise.

## Structure

- `rfc9420.txt` - Complete RFC 9420 specification (core MLS protocol)
- `rfc9750.txt` - Complete RFC 9750 specification (MLS architecture)
- `nip-ee-implementation.md` - Implementation notes for NIP-EE (Nostr MLS spec)
- `nostr-mls-integration.md` - Integration details for the rust nostr-mls crate
- `protocol-flows/` - Detailed protocol flow diagrams and explanations
- `security-analysis/` - Security considerations and threat model analysis
- `implementation-notes/` - Project-specific implementation decisions and rationale

## Quick Reference Links

### Official Specifications
- [RFC 9420 - MLS Protocol](https://www.rfc-editor.org/rfc/rfc9420.html)
- [RFC 9750 - MLS Architecture](https://www.rfc-editor.org/rfc/rfc9750.html)
- [NIP-EE](https://github.com/nostr-protocol/nips/blob/master/EE.md)

### Implementation Resources
- [nostr-mls Rust crate](https://github.com/erskingardner/rust-nostr)
- [MLS Working Group](https://datatracker.ietf.org/wg/mls/about/)

## For LLMs and AI Tools

This documentation is structured to help AI coding assistants understand the MLS protocol context when working on this project. Key areas to reference:

1. **Protocol Understanding**: Start with `protocol-flows/` for message sequence diagrams
2. **Implementation Details**: Check `nostr-mls-integration.md` for library-specific patterns
3. **Security Context**: Review `security-analysis/` for threat considerations
4. **Code Patterns**: See `implementation-notes/` for project conventions

When modifying MLS-related code, always consider the security implications and protocol requirements documented here.
