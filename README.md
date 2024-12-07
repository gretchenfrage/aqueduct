
# Aqueduct: Stateful network protocols done right

Aqueduct is a protocol based on sending networked message channels down other
networked message channels, based on QUIC.

Rather than competing with stateless protocols like HTTP and gRPC, where the
creation of connections is considered an implementation detail, we aim to
create a great experience for stateful network protocols, such as video games.

Our goal is to take the experience of programming with async mpsc channels, and
extend it into a distributed context via simple, flexible, high-performance
primitives.

## Specification

See [PROTOCOL.txt].

## Design principles

See [docs/PRINCIPLES.md] for more elaboration.

1. **Tractable primitives over automagic black boxes.**
2. **Clear, language-agnostic protocol specification.**
3. **Exploit the strengths of async Rust.**
4. **Exploit the strengths of QUIC.**
5. **Support stateful logic.**
6. **Make application logic composable and localized.**
7. **Facilitate low-latency designs.**
8. **Facilitate secure designs.**
9. **Facilitate fault-tolerant designs.**

## Example

TODO

## More resources

See the [docs] subdirectory.
