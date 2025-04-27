
<img align="right" height="75" src="docs/.assets/aqueduct.png"/>

# The Aqueduct network protocol

Aqueduct is a protocol based on sending networked message channels through
other networked message channels, built on QUIC. We believe that async tasks
communicating via async channels is a powerful architecture. For example,
[it can be used to simulate actor oriented programming][1]. Our goal is to take
this experience, which Rust already excels at, and elevate it to a distributed
context.

[1]: https://ryhl.io/blog/actors-with-tokio/

Rather than competing with stateless protocols like HTTP and gRPC, where the
creation of connections is considered an implementation detail, we aim to
create a great experience for stateful network protocols, such as video games,
stateful stream processing, actor-oriented programming, reactive programming,
and more.

## Documentation

- [`OVERVIEW.md`](OVERVIEW.md): Overview of the Aqueduct protocol's design and functionality.
- [`PROTOCOL.md`](PROTOCOL.md): Protocol reference for creating an Aqueduct implementation.

## Design principles

1. **Tractable primitives over automagic black boxes.**
2. **Clear, language-agnostic protocol specification.**
3. **Exploit the strengths of async Rust.**
4. **Exploit the strengths of QUIC and TLS.**
5. **Support stateful logic.**
6. **Make application logic composable and localized.**
7. **Facilitate low-latency designs.**
8. **Facilitate secure designs.**
9. **Facilitate fault-tolerant designs.**

## Example

TODO
