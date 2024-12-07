
# Aqueduct design principles

TODO

1. **Tractable primitives over automagic black boxes.**
2. **Clear, language-agnostic protocol specification.**
3. **Exploit the strengths of async Rust.**
4. **Exploit the strengths of QUIC.**
5. **Support stateful logic.**
6. **Make application logic composable and localized.**
7. **Facilitate low-latency designs.**
8. **Facilitate secure designs.**
9. **Facilitate fault-tolerant designs.**



<!--
1. **Give tractable primitives, not automagic black boxes.**

   _Some protocols ask the user to conform their program's architecture to
   highly complex and particularized abstractions, and promise to automagically
   do things like peer-to-peer routing in return._

   _Instead, we seek to give the user simple building blocks with obvious
   behavior that can be composed together to create higher level behavior in
   exactly the way the user needs it to happen._

2. **Have a clear, language-agnostic protocol specification.**

   _It is not enough to technically have a specification as an afterthought.
   We must have trivially findable, self-contained documents that would suffice
   on their own to create a third party implementation in a random language._

3. **Exploit the strengths of async Rust.**

   _Although we commit to making sure Aqueduct can be implemented in any
   language, Rust and its async ecosystem has many powerful constructs, and we
   can make design decisions based on having a high level of synergy with them._

4. **Exploit the strengths of QUIC.**

   _Aqueduct runs on top of the QUIC protocol, which is a UDP-based alternative
   to TCP. However, QUIC is less so "TCP but faster," and more so "TCP but with
   more degrees of control." Taking a TCP based protocol and swapping in QUIC
   as a drop-in replacement is unlikely to improve performance. Rather, we aim
   to exploit the unique strengths of QUIC on a deep design level._

5. **Prioritize supporting stateful application logic.**

6. **Make composable application logic natural.**

7. **Make round trip-minimizing designs natural.**

8. **Make secure designs natural and have clear security properties.**

9. **Make fault tolerant designs natural.**

-->