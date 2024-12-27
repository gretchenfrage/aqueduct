The Aqueduct Protocol Specification

Pre-Release Version: 0.0.0-AFTER

---

# About this document

For pre-1.0 versions of Aqueduct, this document is given the same
version number as the package itself. Once the package and/or protocol
hits version 1.0, version numbers for the package and for the protocol
will become independent.

This is a text file with a maximum width of 73 characters. It should be
self-sufficient as a reference for someone creating their own Aqueduct
implementation.

Any code changes that change the protocol should correspond to updates
to this document to keep it up-to-date. We will become more conservative
and intentional about changing the protocol once we hit 1.0.

We use the "-AFTER" suffix on our semantic versions to denote that the
current version of the code / specification belongs to some commit after
the release-tagged commit corresponding to that version of the code, and
before the next release-tagged commit corresponding to the following
version.

Pre-1.0 versions of this document may include TODO comments with notes
on ways we would like to explore potentially enhancing the protocol. A
TODO comment noting some idea does not constitute a commitment or
promise to actually going through with it.

# Overview

This is a non-normative section that attempts to give the reader a
better understanding of how everything in the Aqueduct protocol fits
together.

## Aqueduct from the user's perspective

An Aqueduct connection wraps around a QUIC connection, and utilizes both
QUIC streams and QUIC datagrams. The main abstraction Aqueduct provides
is a "channel" of messages within a connection. These messages can be
conveyed in the same QUIC stream, to deliver them in a reliable and
ordered fashion, or in many different QUIC streams, to deliver them in
a reliable but unordered fashion in exchange for reduced head-of-line
blocking, or in QUIC datagrams, to deliver them in an unreliable and
unordered fashion, which can be appropriate for real-time media
streaming or other optimized use-cases.

The sender half of an Aqueduct channel can "finish" the channel,
representing a graceful end to the data sent on the channel.
Alternatively, the sender half can "cancel" the channel, representing an
as-soon-as-possible abandonment of all sent sent on the channel, even
currently buffered messages. This is implemented in an efficient way by
using QUIC's stream resetting functionality

An Aqueduct connection starts with a single "entrypoint channel", going
from the client to the server. Subsequent channels, in either direction,
are created by "attaching" a sender or receiver for the channel to a
message sent on a different channel. This is analogous to how, when
using message channels within a process, such as within Tokio, one can
send a message that has other senders and receivers as fields within it.
Aqueduct implementations facilitate serialization systems that know not
only how to (de)serialize the binary payload of a message, but also how
to attach/detach sender/receiver attachments from the message, to create
a seamless experience for networked channels.

When the Aqueduct client and server initialize, their applications have
the ability to exchange multimaps of headers, similar to HTTP or Email
headers. The addition of this feature is inspired by Gordon Brander's
essay "If headers did not exist, it would be necessary to invent them"
[1]. The main expected use case is for the client and server's
serialization middleware to use headers to agree on a serialization
format. They can be useful for other things too, such as telemetry.

[1]: https://newsletter.squishy.computer/p/if-headers-did-not-exist-it-would

## The distributed algorithm

### Why this is non-trivial

If the QUIC connection as a whole is lost, then the Aqueduct connection
is lost too. This doesn't complicate the Aqueduct protocol much.
"Exactly-once message delivery is impossible in a distributed system"--
coping with this fact is in the domain of opinionated, higher-level
abstractions that Aqueduct itself can't and shouldn't try to do the work
of. We do, however, have to be mindful of behavior where only part of
the Aqueduct connection can fail despite the connection as a whole
remaining.

Aside from the obvious case of the connection as a whole going down,
there are various ways a message sent can fail to be delivered: if the
channel it was sent on was cancelled, or if it was intentionally sent in
an unreliable datagram and then the datagram was lost, or if the
receiver was dropped. This can cause complications if the lost message
had new channels attached to it. If a message with an attached channel
is lost, then all messages sent in that new channel are lost. Those
message could, in turn, have more channels attached to them. These
complications are exacerbated by the facts that 1. messages sent on a
channel can arrive before the message the channel's receiver was
attached to arrives 2. QUIC guarantees no time bound on how late a
stream or datagram can arrive.

### Channel IDs

Each channel is identified by a channel ID, which encodes the direction
the channel is flowing (which side of the connection has the sender and
which the receiver), as well as which side created the channel. In
combination, these reveal whether the creating side created the channel
by sending a sender to the other side, or by sending a receiver to the
other side.

### Channel control stream

At a given point in time, the state stored by either side of the
connection includes a mapping from channel ID to sender or receiver
state for that channel. There is a mechanism by which the sender state
on one side and the receiver state on the other side for a certain
channel become connected by a bidirectional QUIC stream, the channel's
"channel control stream", regardless of how the messages are conveyed.
This is, of course, subject to edge cases.

One function of the channel control stream is for the sides to agree on
what exact set of messages sent were or were not delivered. This
includes not just acks, but committing nacks, where a receiver not only
states that a message has not been processed, but commits to never
processing it (in a certain sense) even if it arrives late.

When one side of the connection creates a channel (it sends a message
with a sender/receiver attached, and locally creates the matching
receiver/sender), it is the other side of the connection that creates
the channel control stream, at the same time that something triggers it
to create its sender/receiver for the channel.

The side that created the channel cannot discard its sender/receiver
state until either its channel control stream appears or the message
its receiver/sender was attached to is declared lost, to ensure proper
shutdown can be performed.

When one side sends a message with a new receiver attached, and locally
creates the corresponding sender, it can begin sending messages on the
new channel immediately. This minimizes latency. However, this also
means that messages it sent on the new channel might arrive before the
message the new channel was attached to does, or even arrive despite
the message the new channel was attached to being lost.

When one side of a connection receives a message sent on a channel for
which the channel was created by the remote side, if the local side does
not currently have a receiver for the channel, it automatically creates
one (and thus creates a channel control stream). The happy path is that
this occurs because the channel-creating message has not yet arrived.
However, it can also be the case that the channel-creating message has
been or will be lost, or that the channel has already shut down and that
the current message is simply arriving late. In the case of late
arrivals, this can result in "ghost receivers" for a channel being
created after the real receiver has already been discarded.

When one side receives the creation of a channel control stream for an
outgoing channel, it attempts to find a local sender that does not yet
have a channel control stream, and installs the new stream as its
channel control stream. This is the happy path.

If that happens but the local sender cannot be found, it assumes that
the channel control stream was created by a ghost receiver, and resets
the channel control stream which triggers the receiver to discard its
state.

If that happens and the local sender can be found, but the local sender
already has a channel control stream, it assumes that the new channel
control stream was created by a ghost receiver in a way that has raced
with the distributed shutdown of the real sender and receiver, and thus
also resets the new channel control stream.

The "ghost receiver" problem doesn't occur for channels where the side
creating the channel attached the sender to another message and created
the receiver locally.

The sender half of a channel continuously tells the receiver the set of
unreliable messages it's sent, via the channel control stream, so that
the receiver can nack unreliable messages that haven't arrived in a
reasonable amount of time. When a sender initiates graceful shutdown, it
tells the receiver the final set of all messages it's sent, so that the
receiver can wait to receive all reliable messages, and wait a
reasonable amount of time to receive all unreliable messages. The
closing of the receiver can be initiated by the channel gracefully
finishing, by the sender cancelling the channel, or by the receiver-side
application. When this occurs, the receiver acks and nacks the final set
of messages received, then discards the receiver. Discarding the
receiver ensures that any late-arriving messages on the channel will be,
at most, enqueued into a ghost receiver which gets discarded after a
round trip. Thus, the effect is similar to nacking all possible future
messages on the channel.

The sender half of a channel also continuously tells the receiver the
set of reliable messages it's sent, and the receiver acks them. However,
this is cheaper and simpler for reliable messages, as reliable messages
won't be nacked until and unless the channel is closed abruptly. The
only reason why it's necessary to ack reliable messages that are sent
reliably before the channel closes is to prevent state necessary to
handle channels lost in transit from growing unboundedly.

### Channels lost in transit

After one side sends a message with an attached receiver/sender, if the
message with the attachment is declared lost (e.g. due to it being
nacked), that side discards its local sender/receiver for the new
channel. If that sender/receiver has a channel control stream attached,
it resets it, triggering the remote receiver/sender to discard its
state. If not, the discarding of the local sender/receiver ensures that
any future attempt by the remote side to create the channel control
stream will be responded to with resetting.

If this occurs specifically with the local side being a sender, the
local side also has to consider lost all messages previously sent on the
sender. This can have a cascading effect, as those messages might have
had receivers attached to them, which would necessitate additional
senders being discarded, and so on and so forth. The state an Aqueduct
implementation has to maintain to achieve this forms tree structures.

For a message sent to have a chance of being delivered to the remote
application, it is not only necessary for the message itself to be
acked, but also, if the local side sent the message on a channel that
the local side created, for the message which created the channel to be
acked, and, if the local side sent that message on a channel that the
local side also created, for the message which created that channel to
be acked, and so on and so forth until this chain terminates either with
1. the entrypoint channel, which is special, or 2. an incoming channel.
The latter case terminates the chain because a message can only be
received from the remote side if the message was sent by the remote
application, which implies that whatever message created that channel
necessarily must have been delivered to the remote application.

Each sender can maintain a boolean variable called "reachable". If this
is true, it indicates that either the corresponding receiver either has
been given to the remote application, or could immediately be taken by
the remote application if it performed the necessary sequence of
dequeueing messages from receivers. For the entrypoint sender as well as
senders received from the remote side, this initializes as true, whereas
for senders created by sending a receiver to the remote side, this
initializes as false.

Whenever a sender is used to send a message which has an additional
sender/receiver attached, the Aqueduct implementation makes a link from
the original sender to the subsequentially locally created
receiver/sender. It keeps this link until either the message is nacked,
or the message is both acked and the original sender's "reachable"
variable becomes true. If the message is nacked, all linked senders and
receivers are reset and discarded. If a linked sender had any links for
any messages whatsoever, those links are also traversed in the same way.
On the other hand, when it becomes true that both a message is acked and
the sender it was sent on becomes reachable, then all linked senders
are marked as reachable, which itself can trigger a recursive process if
those senders already have acked messages. It is not necessary for a
"reachable" variable to be held for receivers.

### Notes on the necessity of lost-in-transit recursive cleanup

If this cleanup process did not exist, these problems would occur:

- Local senders/receivers would never be garbage collected if the remote
  receiver/sender was lost in transmit.
- Messages sent on a sender for which the remote receiver was lost in
  transit would trigger the creation of an unusable remote receiver
  which would never be garbage collected.

If this cleanup process did exist, but in a more limited form, without
the "reachable" variable or properly implemented recursive cascading,
such that a message with attachment's links were always discarded
immediately upon it being acked, it would actually sort of technically
work, but it would be problematic. The cascading of noticing that there
was an upstream loss-in-transit from a sender with previously acked
messages with attachments to its attached senders/receivers would still
occur, since the fact that the sender had acked messages implies that
its control stream is already attached, which means it would reset its
control stream, triggering the remote side to discard its enqueued
messages, triggering the destructors for any attachments they had
attached, triggering the closing of the channels from the remote side,
which the local side would eventually observe. However, this would
create suboptimal latency, and require suboptimal amounts of network
communication. Moreover, it's conceivable that if one side were
recursively expanding chains of senders faster than this RTT-limited
emergent failure could collapse them, they could effectively outpace
the system's ability to detect that they were lost in transmit at an
upstream point, which could potentially be a nasty emergent failure
case.

### Closed channel loss-in-transit detection

It is possible for a sender or receiver to be detected as
lost-in-transit after the local side has already initiated and completed
a shutdown of the channel, resulting in the channel control stream
having already deinitialized. If such a "closed channel lost" scenario
occurs, this is conveyed with a message on a different stream, which
lets the remote side discard any state it is still be holding for the
channel in the hopes of handing it off to the remote application.

### Errors for lost versus ungracefully closed channels

It would be possible, and in some ways easier, for the error for a
channel being lost in transit to be the same as the error for the sender
or receiver being dropped. The reason why they are distinguished is to
make debugging less confusing.

Some examples for losing a receiver are:

- If a receiver is dropped after being dequeued from another receiver,
  the remote sender gets a "receiver dropped" error.
- If a receiver is lost in transit, and thus could not be dequeued from
  another receiver even if the application drained it empty, the remote
  sender gets a "lost in transit" error.
- If a receiver is available to be dequeued from another receiver, and
  then the application drops the outer receiver without dequeueing the
  inner receiver, the sender for the inner receiver does still get a
  "receiver" dropped error rather than a "lost in transit" error.

We'll assume that dropping a sender in these cases causes it to be
cancelled. Some examples for losing a sender are:

- If a sender is cancelled after being dequeued from a receiver, the
  remote receiver gets a "cancelled" error.
- If a sender is lost in transit, and thus could not be dequeued from a
  receiver even if the application drained it empty, the remote receiver
  gets a "lost in transit" error.
- If a receiver is available to be dequeued from another receiver, and
  then the application drops the receiver without dequeueing the sender,
  the remote receiver does still get a "cancelled" error rather than a
  "lost in transit" error.

When resetting a stream, the error code used in the reset is used to
convey whether this reset is due to loss-in-transit or not. The
processing of a stream reset can differ based on the reset code in 2
ways:

- The reset code can be threaded through to application-facing errors
  and other resets.
- If the reset code indicates a loss, enqueued state which is waiting
  to be taken by the application is discarded with the understanding
  that the application would never be able to take it.

There are some situations where a "lost" reset code has to be used to
prevent a memory leak on the remote side. There are other situations
where this is not the case, but using a "lost" reset code is still best
to ensure the remote application receives an appropriate error message.

### Loss of message after ack

It's possible for a message to be received, deserialized, and enqueued
for delivery to the application, then dropped before the application has
dequeued it. This can happen due to the channel being cancelled, or due
to old buffered messages being automatically evicted from an unreliable
channel. This can also occur for messages after they're enqueued for
transmission but before they're transmitted or even serialized.

Ideally, an Aqueduct implementation tries to make this appear to the
other half as a "lost in transit" error rather than the error that would
occur due to a channel handle being dropped. This might be achievable
via thread-local variables, set in the channel implementation and read
by the sender/receiver's destructors.

# Specification

This is a normative section to be used as a reference when implementing
the Aqueduct protocol.

## QUIC

Aqueduct runs on top of QUIC. In this document, "streams" refers to QUIC
streams, whereas "channels" refers to Aqueduct channels. QUIC datagrams
are also used.

Servers and clients must accept datagrams (they must have enabled the
QUIC unreliable datagram extension first standardized in RFC 9221). If
they do not, they are not compliant Aqueduct implementations, and this
must be treated as a protocol error.

TODO: Fallback to TCP or WebSocket for situations where the network
      refuses to carry UDP packets.
TODO: Fallback, or completely convert to WebTransport for running within
      web frontends more performantly than falling back to WebSocket.

## ZeroRTT

An Aqueduct client may send any data in 0-RTT streams and 0-RTT
datagrams. It must buffer all data sent on 0-RTT streams until it learns
the server accepted or rejected its 0-RTT data. If the server rejects
its 0-RTT data, it must retransmit all data it sent on a 0-RTT stream on
a new 1-RTT stream. It should not buffer and retransmit datagrams.

If a handle to a stream opened in 0-RTT mode is being held somewhere in
Aqueduct's state, and Aqueduct is forced is re-transmit the buffered
data in a new 1-RTT stream as described above, it must replace the old
stream with the new one at the place where it's storing it in its state.
Anything in the Aqueduct protocol which states something along the lines
of some data only being allowed to be sent once, does not apply to the
retransmission of 0-RTT data in a 1-RTT stream as described above.

Clients should remember address validation tokens from NEW_TOKEN frames
and use them when making 0-RTT requests if able. Transmitting 0-RTT data
will not be beneficial if the client does not have an address validation
token, so a client may avoid bothering to do so if it doesn't.

An Aqueduct server may receive data from 0-RTT streams, but it must not
process it until it can be sure it is not coming from a replay attack.
If the client used some un-guessable and un-forgeable address validation
token, and the server maintains state capable of detecting token reuse,
and this system is guaranteed to experience no false negatives with
regard to detecting token reuse, and from this system the server knows
that the client's address validation token has never been used before,
then the server may conclude that the 0-RTT data is definitely not
coming from a replay attack, and process it immediately. In such a
system, the server must take care to ensure that this token reuse
detection state is at least as persistent as whatever cryptographic keys
or other mechanism it is using to prevent its tokens from being guessed
or forged. The server also must ensure that there are also no
possibilities for false negatives introduced by problems relating to
token encryption keys being shared by servers in a server farm or
cluster, or by eventual consistency in replay detection state being
shared, replicated, sharded, or otherwise distributed across different
servers in a server farm or cluster, or by lack of consistency in the
face of un-graceful server shutdown, or by systems to copy or roll back
disk state, or by anything else. If the server is using validation
tokens to protect against replay attacks as such, it also of course must
remember to actually check whether a connection is validated. If the
server cannot for any reason determine with total confidence that 0-RTT
data is not coming from replay attacks, it must either reject the 0-RTT
data on the TLS level (QUIC/TLS APIs may not always make this possible),
or wait until the TLS handshake fully completes before processing that
data, at which point the completion of the TLS handshake proves the
authenticity of the 0-RTT data.

An Aqueduct server always may send data as 0.5-RTT data, and should
do so if it is or may be processing 0-RTT data.

TODO: Conveying that data was sent in 0-RTT and getting responses of
      whether it was accepted or rejected without relying on TLS API
      itself, to facilitate proxies / reverse proxies and/or limited
      TLS APIs.
TODO: Conveying the proof-of-no-replay token through some other field
      than that address validation token, to deal with limited TLS APIs.
TODO: TLS client authentication, both in general, and also getting that
      to work security in 0.5-RTT by encoding some information about a
      previously authenticated session in the token. Consider security
      issues with a client, potentially a proxy, reusing a token but
      not actually meaning to authenticate its further requests with the
      old client key. Consider whether this has tradeoffs with
      cryptographic forward security and how to navigate those.

## Encoding

### Endianness

Values are encoded little-endian unless stated otherwise.

### Var len ints

Sometimes, a variable length uint encoding is used. An encoded var len
int always contains at least 1 byte. The lowest 7 bits of the byte
encode the lowest 7 bits of the represented uint, and the highest bit of
the byte is a 1 if there is at least 1 more byte in the encoded var len
int. If there is another byte in the encoded var len int, the lowest 7
bits of that byte encode the next lowest 7 bits of the represented uint
(so, the 8th through 14th lowest bits), and the highest bit represents
whether there is yet another byte in the encoded uint. This pattern
continues until terminated by an encoded byte with its highest bit being
0. It must be considered a protocol violation if a var len int is
encoded in more bytes than necessary, or if it contains more than 64
bits, excepting unavoidable trailing 0 bits.

### Byte arrays

Sometimes, a var len byte array is encoded. This is encoded as a var len
int, encoding the length of the array, following by that many bytes.

### Header data

Sometimes, "header data" is encoded. This is encoded as a var len byte
array. Within the outer var len byte array are 0 or more inner var len
byte arrays encoded back-to-back. It is a protocol error if there is an
odd number of inner byte arrays. Each sequential group of 2 inner byte
arrays is a key/value pair. Keys are not required to be unique; header
data is a multimap. It is a protocol error if a key is not an ASCII
string. A value may be any sequence of bytes. It is a protocol error if
a key is empty. A value may be empty.

#### Recommendations for users utilizing headers

The recommended way to choose a name for a header is to use a short but
descriptive name, followed by a dash, followed by a random 6-digit
hexadecimal sequence generated at the time of designing the header by
an website or by a command such as `openssl rand -hex 3`.

For example, one might design a serialization middleware that utilizes a
header key such as `cbor-b3b650` to indicate that CBOR is used to
serialize the messages, wherein the value is some CBOR-encoded settings
object.

The addition of random hex bytes serves as a decentralized way to avoid
accidentally colliding with some other engineer in the world who wants
to call their header `cbor`.

If one already is using a header with some generated random byte
sequence, and wants switch the semantics of their header in a
non-backwards compatible way, it is recommended that one add or
increment a version number at the end of their header name, such that
the key might become something like `cbor-b3b650-v4` or
`cbor-b3b650-0.4.0`. This could be used to indicate, for example, that
the expected fields of the value are different.

The important part is to avoid excessively generating new hex sequences,
which keeps the probability of accidental collision low.

If one wants to develop code that uses an established header in some
experimental way that's not yet standardized, it's recommended that one
add a suffix that would prevent such code from being misinterpreted by
prod code, such as `cbor-b3b650-TEST` or `cbor-b3b650-0.4.0-AFTER`.

### Pos-neg range data

Sometimes, "positive negative range data" is encoded. This is encoded as
a var len byte array which contains 0 or more var len ints encoded
back-to-back. The range of ints from the "start" (inclusive) to the
start plus the 1st element is considered "positive," and the range of
ints from the start plus the 1st element to that plus the 2nd element is
considered "negative", and the range of ints from that to that plus the
3rd element is considered "positive" again, and the range of ints from
that to that plus the 4th element is considered "negative" again, and so
on and so forth. The meaning of "positive" and "negative" as well as
what the "start" is depends on context. It is a protocol error if any
int in the sequence is 0, with the exception that the 1st int in the
sequence may be 0 if there are also other ints after it.

### Channel IDs

Each (networked) channel and oneshot channel within a connection has a
64-bit channel ID.

- The lowest bit is a 0 if the channel is flowing from the client to the
  server, and 1 if it is flowing from the server to the client.
- The second lowest bit is a 0 if the channel ID was minted by the
  client, and a 1 if it was minted by the server.
- The third lowest bit is a 0 if the channel is not a oneshot channel,
  and a 1 if it is a oneshot channel.
- The other 61 bits are the "channel idx", a 61-bit uint.

When channel IDs are minted, the side minting them assigns them channel
idxs seqeuentially within their index space, which is defined by the
first 3 bits, starting at 0.

The channel ID which consists entirely of zeroes (flowing towards
server, created by client, not oneshot, index 0) is considered the
"entrypoint channel" and treated specially in some cases.

Channel IDs are encoded as var len ints.

### Frames

Frames are the unit of the Aqueduct client and server sending each other
self-contained messages on the wire.

Each frame begins with a "frame type byte", a single byte indicating
what type of frame it is. Then, it may have further bytes, in accordance
with logic specific to its frame type.

It is clear from a byte sequence that begins with a frame when that
frame ends. Thus, it is possible to encode multiple frames back-to-back
without additional framing.

The client and server must listen to each other for bidirectional QUIC
streams, unidirectional QUIC streams, and QUIC datagrams. Both the data
in a stream and the data in a datagram is a sequence of one or more
frames encoded back-to-back.

If the data in a stream or in a datagram fails to decode as frames,
that's a protocol violation. If there's extra bytes after the frames,
that's a protocol violation, although it's unclear what that would even
mean as both streams and datagrams are allowed to contain multiple
frames and frames describe their own length implicitly. If a received
QUIC stream is elegantly finished without at least one full frame being
received on it, that's a protocol violation. If a received datagram does
not contain at least one full frame on it, that's a protocol violation.

If multiple frames are received in the same stream or datagram, they
must be processed in sequence (that is, one after the other, in the
order they are encoded). To clarify, if a frame results in a message
being delivered to the application through a channel, there is no
requirement that the application must process these messages in an order
consistent with the order the frames were encoded within the same stream
or datagram. However, there would be a requirement that the Aqueduct
implementation enqueue these messages for delivery to the application in
an order consistent with the order of the frames themselves, as the
processing of a frame which triggers enqueueing a message into an
application-facing channel would encompass just the enqueueing of the
message for delivery to the application, and not the dequeueing and
processing of the message by the application. Frames are abstracted away
from the application.

Frames received from different streams or different datagrams may be
processed in parallel.

Parallelizing parts of the processing of frames within the same stream
or datagram is permissible only to the extent that it has no possibility
of meaningfully changing behavior / introducing race conditions.

If a QUIC stream is reset, the receiving side should disregard any
partially received frames up to the point of resetting, and does not
have to process any frames previously received from that stream if it
has not already done so.

## Frame types

The following frame type bytes and corresponding frame types exist:

- 239: Version
- 1: ConnectionControl
- 2: ChannelControl
- 3: Message
- 4: SentUnreliable
- 5: AckReliable
- 6: AckNackUnreliable
- 7: FinishSender // NEEDS WORK: rename to like, FinishSender, CloseReceiver
- 8: FinalAckNack
- 9: ClosedChannelLost

### Version frames

A version frame is encoded as:

- The frame type byte: 239
- The magic byte sequence: 80, 95, 166, 96, 15, 64, 142

  The frame type byte and the magic byte sequence were both chosen
  randomly, to help avoid collision with non-Aqueduct protocols.
- Human text: The bytes of the ASCII string "AQUEDUCT".

  This is designed to be a human-readable hint to someone looking at the
  decrypted bytes on the wire as to what sort of traffic this is.
- Version: A var-len byte array containing the ASCII string
  "0.0.0-AFTER".

It is a protocol error for a Version frame to occur elsewhere than as
the first frame in its stream or datagram.

### ConnectionControl frames

A ConnectionControl frame is encoded as:

- The frame type byte: 1
- The client or server's header data: Header data

### ChannelControl frames

A ChannelControl frame is encoded as:

- The frame type byte: 2
- The channel ID: A channel ID

It is a protocol error for a ChannelControl frame to occur elsewhere
than as the first frame in a bidirectional stream in the direction
flowing away from the side that created the stream.

### Message frames

A Message frame is encoded as:

- The frame type byte: 3
- The channel ID of the channel the message was sent on: A channel ID
- Message number: A var len int

  Within each channel, there are 2 spaces of message numbers, one for
  reliable messages, and one for unreliable messages. Message numbers
  are assigned sequentially by the sender starting at 0 within the space
  defined by the channel and whether the Message frame is being written
  to a stream or a datagram.
- The message payload: A var len byte array
- The message attachments: A var len byte array, containing 0 or more
  channel IDs encoded back-to-back.

It is a protocol error for a Message frame to occur in a bidirectional
stream.

### SentUnreliable frames

A SentUnreliable frame is encoded as:

- The frame type byte: 4
- The number of messages sent unreliably on this channel since the last
  SentUnreliable frame on this channel:  A var len int

It is a protocol error for a SentUnreliable frame to occur elsewhere
than in a channel control stream in the sender-to-receiver direction.

### AckReliable frames

An AckReliable frame is encoded as:

- The frame type byte: 5
- The acks: Pos-neg range data, positive ranges represent acks, negative
  ranges represent messages not yet being acked or nacked rather than
  them being nacked, and the start is the highest message number in the
  channel's reliable message space for which all message numbers less
  than it have been acked.

It is a protocol error for a AckReliable frame to occur elsewhere than
in a channel control stream in the receiver-to-sender direction.

### AckNackUnreliable frames

An AckNackUnreliable frame is encoded as:

- The frame type byte: 6
- The acks and nacks: Pos-neg range of data, positive ranges represent
  acks, negative ranges represent nacks, and the start is where the last
  AckNackUnreliable for this channel left off.

It is a protocol error for a AckNackUnreliable frame to occur elsewhere
than in a channel control stream in the receiver-to-sender direction.

NOTE: one reason AckNackUnreliable frames don't have the ability to
      represent a not-yet-acked-or-nacked state in the way that
      AckReliable frames do is because it may be reasonable in some
      circumstances for a higher priority reliable QUIC stream to starve
      a lower priority reliable QUIC stream for arbitrarily long amounts
      of time, whereas QUIC datagrams are all considered to be of
      equally maximal priority.

### FinishSender frames

A FinishSender frame is encoded as:

- The frame type byte: 7
- The number of messages ever sent reliably on this channel: A var len
  int

It is a protocol error for a FinishSender frame to occur elsewhere than
in a channel control stream in the sender-to-receiver direction, and as
the final frame in that stream in that direction before it finishes or
resets.

### FinalAckNack frames

A FinalAckNack frame is encoded as:

- The frame type byte: 8
- Final reliable acks and nacks: Pos-neg range data, positive ranges
  represent acks, negative ranges represent nacks, and the start is the
  highest message number in the channel's reliable message space for
  which all message numbers less than it have been acked.
- Final unreliable acks and nacks: Pos-neg range of data, positive
  ranges represent acks, negative ranges represent nacks, and the start
  is where the last AckNackUnreliable for this channel left off.

It is a protocol error for a FinalAckNack frame to occur elsewhere than
as the final frame in a channel control stream in the receiver-to-sender
direction.

### ClosedChannelLost frames

A ClosedChannelLost frame is encoded as:

- The frame type byte: 9
- The channel ID: A channel ID

It is a protocol error for a ClosedChannelLost frame to occur elsewhere
than in the connection control stream.

## Reset error codes

The following error codes might be used when resetting a QUIC stream:

- 1: "sender cancelled"
- 2: "sender lost"
- 3: "receiver dropped"
- 4: "receiver lost"

## Connection control stream

When the Aqueduct client creates the Aqueduct connection, it creates a
QUIC connection to the server. As soon as it can, it opens up a
bidirectional stream, stores it as the "connection control stream", and
sends on it a Version frame, followed by a ConnectionControl frame.

When the server observes the QUIC connection opening, it waits to
observe the opening of a bidirectional stream followed by the receiving
on that stream of a Version frame followed by a ConnectionControl frame.
Once this occurs, the server stores that stream as the connection
control stream, and sends on it a Version frame, followed by a
ConnectionControl frame.

The server may branch on the Version and ConnectionControl data it
receives from the client in determining what content to send in its
Version and ConnectionControl response. The server may allow the
application to read the client's connection headers and then determine
based on that what connection headers the server has for this
connection.

The client may begin sending message frames to the server before it
receives the server's connection headers. This avoids adding a round
trip to connection start-up time. However, until the client receives the
server's connection headers, it must encode a Version frame as the first
frame on any stream or datagram on which it is encoding other frames.
This helps protect the client from accidentally sending data to a server
which is not an Aqueduct server in a way the server could misinterpret,
as a Version frame begins with a shibboleth magic byte sequence.

The server does not have a reason to send any data to the client, on
streams or datagrams, before receiving the client's connection headers.
It must not do so. If in some future version of this protocol the server
gains a reason to do so, care would have to be taken regarding the same
concern mentioned in the last section.

If the server receives frames other than Version and ConnectionControl
in any way before it receives the client's ConnectionControl frame, it
must wait to process them until it receives the client's
ConnectionControl frame. This might occur due to race conditions between
the client sending data on the connection control stream and the client
sending data by other means.

A connection only has 1 connection control stream, and it lasts as long
as the connection. It is a protocol error if the connection control
stream is reset or finished. The client and the server both send a
ConnectionControl frame exactly once, on the connection control stream.
The server identifies the connection control stream by it being a
bidirectional stream opened by the client that begins with a Version
frame followed by a ConnectionControl frame. It is a protocol error if
either side sends a ConnectionControl frame multiple times. It is a
protocol error if the client sends a ConnectionControl frame on anything
other than a bidirectional stream, as the second frame on that stream,
wherein the first frame on that stream is a Version frame. It is a
protocol error if the server sends a ConnectionControl frame on anything
other than the control stream, as the second frame on that stream,
wherein the first frame on that stream is a Version frame.

## Sending and receiving messages

### Senders and receivers

At a given point in time, the client and the server both have a set of
senders and receivers. Each sender/receiver uniquely corresponds to a
channel ID. It is a protocol violation if something would trigger the
creation of a sender on the client for which the channel ID indicates
that the client should be the receiver, or vice versa for receivers, or
vice versa for servers.

When the client first initializes, it begins with a single sender, for
the entrypoint channel, and no receivers. When the server first
initializes, it begins with no senders or receivers. After the server
processes the client's ConnectionHeaders, it creates a receiver for the
entrypoint channel.

### Sending a message

On either side, the application can send a message on a channel for
which a sender exists on that side. It does so by sending a Message
frame. It may send multiple message frames on the same unidirectional
stream to send them in an ordered fashion, or on different
unidirectional streams to send them in an unordered fashion, or in
datagrams to send them in an unreliable fashion.

An Aqueduct implementation must provide an API for the application to
send messages on local senders. The application must be given the
ability to control the message's binary payload, and also to create new
channels by having their sender or receiver attached to the message at a
particular index within the message's array of attachments.

When the sender creates a new channel to attach the new channel's sender
or receiver to a message, it must mint a new channel ID for the new
channel. Also, it must locally create a local receiver or sender.

After a sender sends a Message frame, it must send a SentReliable or
SentUnreliable frame indicating that that relevant message number was
sent within a reasonable amount of time, such as 0.1 seconds, or half
the estimated RTT. It may wait for such a delay in anticipation of more
messages potentially being sent soon, in the hopes that only a single
"sent" frame would have to be sent to cover a larger range of packet
numbers.

If a SentReliable or SentUnreliable frame cannot be sent because the
sender's channel control stream is not attached, it must be sent once
the sender's channel control stream becomes attached.

NEEDS WORK closing

### Routing a received message

When a Message frame is received, the side attempts to find an existing
receiver for the channel the message was sent on. If a receiver locally
exists for the message's channel ID, it must route the message to it. If
a receiver does not locally exist for the message's channel ID, and the
channel ID was minted by the remote side, it must create a new receiver
for the channel ID, and route the message to it. If a receiver does not
locally exist for the message's channel ID, and the channel ID was
minted by the local side, it must discard the message without processing
it.

An Aqueduct implementation may keep a record of recently discarded
receivers, and drop a received Message frame without processing it if
it was sent on a channel for which the receiver was recently dropped.
Such a filter may have false negatives, but not false positives.

### Processing a routed message

Once the Message frame has been routed to its receiver, the receiver
must process it.

If the receiver has previously sent a nack for the message, it must not
process it beyond detecting that it has been nacked and then ceasing
further processing. This requirement is satisfied vacuously for messages
sent reliably, since nacking of reliable messages always corresponds to
discarding the receiver. This requirement does not apply to nacks
potentially sent by other local receivers that previously existed with
the same channel ID (see notes on "ghost receivers").

For each sender/receiver attached to the message, a sender/receiver must
be created locally. If a sender was attached for which a local sender
already exists, that implies that the same sender was attached to
multiple messages, which is a protocol error. The same implicature does
not hold for receivers. It is a protocol error if message's attached
channel IDs indicate that they were minted by the side that received
them.

The message must be conveyed to the local application. This may be done
by enqueueing it to an application-facing queue. Application-provided
deserialization middleware may run at enqueueing time or at dequeueing
time. The application must have the ability to discern what channel the
message came from. The application must be given the message's binary
payload. For each attached sender, the application must be given the
ability to send messages on its channel, and to finish or cancel its
channel, and to observe channel error states. For each attached
receiver, the application must be given the ability to receive messages
from the channel, to observe the finishing of the channel, to abandon
the receiver, and to observe channel error states. For each attachment
conveyed to the application, the application must have the ability to
tell its index within the message's array of attachments.

Conveying attached senders and receivers to the application can be done
by giving the application some sort of sender handle or receiver
handles. When this is done, care should be taken to mitigate resource
leak hazards caused by the presence of attachments that the message
receiver did not expect.

If local sender or receiver experiences a handle being taken from it to
be given to an application multiple times, this implies that the sender
or receiver was attached to multiple different messages, which is a
protocol error.

## Channel control

### Creating senders, receivers, and the channel control stream

When a sender or receiver is created locally, if the channel ID was
minted by the remote side, the local side must create a bidirectional
stream, attach it as the sender/receiver's channel control stream, and
send on it a ChannelControl frame. If the channel ID was minted by the
local side, the sender/receiver initializes without a channel control
stream.

When a side receives a ChannelControl frame, if a local sender/receiver
exists with its channel ID, and the local sender/receiver does not
currently have a channel control stream attached, it must attach the
stream the ChannelControl frame was received on as the local
sender/receiver's channel control stream. It is a protocol error if the
ChannelControl frame is received from something other than a
bidirectional QUIC stream. It is a protocol error if the ChannelControl
frame has a channel ID that indicates that it was minted by the remote
side. If a local sender/receiver does not exist with its channel ID, the
stream must be reset with a "sender lost"/"receiver lost" error code. If
a local sender/receiver exists with its channel ID, but it already has a
channel control stream attached, it must be reset with a
"sender lost"/"receiver lost" error code.

### Acking and nacking

When a receiver processes a reliable message (a Message frame received
from a stream), it may ack it by sending an AckReliable frame on the
control stream. The Aqueduct implementation must ack a message within a
reasonable amount of time after processing it, such as within 1 second.
Excessive waiting risks exacerbating sender-side memory usage, and
potentially triggering the sender to throttle the connection. 

TODO throttling

When a receiver processes an unreliable message (a Message frame
received from a datagram), it may ack it by sending an AckNackUnreliable
frame, so long as it has not previously nacked it. When a receiver
receives a SentUnreliable frame from the channel control stream
indicating that the sender sent some additional unreliable messages, the
receiver must ack or nack all of them within a reasonable amount of time
after receiving the SentUnreliable frame, such as 1 second or twice the
estimated RTT. Excessive waiting risk being directly apparent to the
remote application as excessive delays in loss detection, as well as
exacerbating sender-side memory usage, and potentially triggering the
sender to throttle the connection.

The Aqueduct implementation may avoid acking a message immediately in
the hopes that further received messages would be possible to ack
simultaneously. A receiver may wait to ack a message for a longer period
of time if it cannot ack the message due to the channel control stream
not yet being attached.

It is a protocol violation to ack or nack a message that has already
been acked or nacked by the same receiver.

## Channel shutdown

### Finishing

The application must be provided an API to attempt to gracefully finish
a channel via its local sender. For a sender to attempt to gracefully a
finish its channel, it must first wait for the channel control stream to
become attached if it is not already attached. Then, the sender must
use the channel control stream to send a SentUnreliable frame declaring
any undeclared unreliable messages, unless all unreliable messages have
already been declared, followed by a FinishSender frame, followed by
finishing the sender-to-receiver direction of the control stream. Then,
the sender must enter the "finishing" state.

A sender must not send any additional messages after entering the
finishing state. If it is possible for the application to request this
be done, an error should be returned to the application.

When a receiver receives a FinishSender frame, the receiver must enter
the "finishing" state. Once in the finishing state, the receiver can
finalize finishing once all declared messages have been acked or nacked.
The receiver must wait for all declared reliable messages to have been
received. The receiver must wait for all declared unreliable messages to
either have been received, or to have been nacked. The receiver must not
nack unaccounted-for unreliable messages immediately merely because it
has entered the finishing state--it must give them a fair chance to
arrive. A receiver in the finishing state should follow similar waiting
logic in terms of nacking unreliable messages as it would if it were not
in the finishing state. Once these conditions are met for a receiver,
the receiver must finalizing finishing.







For a sender to initiate an attempt to
gracefully finish its channel, it must send a SentUnreliable frame


 When a sender attempts to gracefully
finish a channel, it must 


An application may attempt to trigger a sender to attempt to gracefully
finish its channel. When a sender attempts to gracefully 
A sender may attempt to gracefully 

## Cascading loss detection

A local sender is either in the "not reachable" state or the "reachable"
state. When a sender is created, if its channel ID was minted locally,
it initializes in the "not reachable" state, with the exception of the
entrypoint sender, which initializes in the "reachable" state. If a
sender is created with a channel ID that was minted remotely, it
initializes in the "reachable" state.


















# Sending messages

On either side, the application can send a message on a channel for
which a sender exists on that side. It does so by sending a message
frame. It may send multiple message frames on the same unidirectional
stream to send them in an ordered fashion, or on different
unidirectional streams to send them in an unordered fashion, or in
datagrams to send them in an unreliable fashion.

It is a protocol error to send message frames on a control stream. It is
a protocol error to send message frames on a bidirectional stream.

# Receiving messages

When a message frame is received and processed, a receiver for its
channel is created if one does not already exist. Then, the message is
enqueued for delivery to the application on that channel.

For every channel attached to the message, the side receiving the
message creates a sender or receiver for the attachment's channel ID,
depending on whether the channel ID indicates that the side that
received the message should have the sender or the receiver for the
attached channel, if such a sender or receiver does not already exist.

When the received message's payload is conveyed to the application, its
attachments (senders and receivers) should also be conveyed with it.
Attached senders should be conveyed in a way that allows the application
to send messages on the channel of the attached sender, and which
facilitates the sender being closed once the application is done with
it. Attached receivers should be conveyed in a way that allows further
messages received on the channel of the attached receiver to be conveyed
to the application such that the application can tell that they have
been received on that channel, and which facilitates the receiver being
closed once the application is done with it.

The Aqueduct implementation must take care to ensure that a received
message which contains senders / receivers which the application did not
expect to be there is not a hazard for causing a resource leak, such as
by making it so that any senders / receivers that the receiving
application's deserialization logic does not claim ownership of are
automatically closed, or that their presence triggers some exception
handling logic that otherwise prevents this from being a problem.

It is a protocol error to attach the same channel to multiple different
messages.

TODO maybe that last thing... should just be straight-up allowed?

# Channel control streams

When the client or server creates a receiver for a channel, it creates
a bidirectional stream, and stores it as that receiver's "channel
control stream". Immediately after doing so, it sends a ChannelControl
frame on the stream, with the channel ID of the channel for which the
receiver was created.

Both sides of the connection listen for ChannelControl frames received
on bidirectional streams. It is a protocol error for there to be a
ChannelControl frame on a unidirectional stream, or in a datagram, or
in the connection control stream, or for there to be multiple
ChannelControl frames in the same channel control stream.

When a ChannelControl frame is received on its bidirectional stream as
such:

- If the side that received the ChannelControl stream currently has a
  sender for that channel ID, it attaches the bidirectional stream as
  that sender's channel control stream.
- If the side that received the ChannelControl stream currently has a
  sender for that channel ID, but the sender already has a channel
  control stream associated with it, it writes a ChannelClosed frame
  back on the bidirectional stream.

  NOT FINISHED is ChannelClosed correct?

  TODO I don't think this rule causes problems, but we should double-
       check whether it is actually necessary / the best design.
- If the side that received the ChannelControl stream does not currently
  have a sender for that channel ID, it writes a ChannelClosed frame
  back on the bidirectional stream.

  NOT FINISHED is ChannelClosed correct?

In the channel-sender-to-receiver direction, a channel control stream
can be finished and it can be reset. However, if it finishes without its
final frame being a FinishSender frame, that's a protocol error.

In the channel-receiver-to-sender direction, a channel control stream
can be finished. It can not be reset; if it is reset, that's a protocol
error. If the channel-receiver-to-sender direction finishes before the
channel-sender-to-receiver direction finishes, its final frame in the
channel-receiver-to-sender direction must be an AbandonReceiver frame,
or that's a protocol error.

# Acks and nacks

In Aqueduct, it is possible for a message frame sent on a channel to not
be received and processed by the receiver, despite the connection as a
whole remaining alive. This can occur due to the sender cancelling the
channel, the receiver closing the channel early, or messages sent
unreliably being lost. The Aqueduct protocol contains a system by which
the receiver of messages can not only ack messages frames it's processed,
but also nack messages frames it's declared lost. Moreover, unlike most
similar systems, nacks experience no false positives, as the receiver
nacking a message actually represents a commitment to never processing
that message, even if it arrives late. Other parts of the Aqueduct
protocol rely on this assurance. Since "exactly once message delivery
is impossible in a distributed system," these systems do not apply to
cases where the QUIC connection as a whole closes.

Each sender maintains two sequential u64 counters for messages it's
sent--one for messages sent reliably (in streams), and one for messages
sent unreliably (in datagrams). A message frame contains its message
number within its appropriate space, which is defined by the channel ID
and whether the message frame is in a stream or datagram.

The sender side of a channel control stream may send SentUnreliable
frames indicating that the highest unreliable message number it has sent
for that channel has increased by a certain amount. When a sender sends
a message unreliably, it must send a SentUnreliable frame indicating
that that unreliable message number was sent within a reasonable amount
of time, such as 0.1 second, or half the estimated RTT. It may wait for
such a delay in anticipation of more unreliable messages potentially
being sent, in the hopes that only a single SentUnreliable frame would
have to be sent to cover a larger range of packet numbers.

When the receiver side of a channel control stream receives a
SentUnreliable frame, it must ack or nack all unreliable messages sent
up to the message number it was raised to after a reasonable loss-
detection delay after the SentUnreliable frame was received, such as 1
second or twice the estimated RTT. It does so by sending an
AckNackUnreliable frame back on the channel control stream.

TODO nagling-inspired thing instead of arbitrarily chosen durations?

When a receiver nacks some unreliably received message, it must retain
whatever state is necessary to ensure that if it does ever receive that
message frame, it will discard it without processing it. If it did, this
would be a protocol error.

An Aqueduct implementation does not need to explicitly harden itself
from the possibility of an unreliable or other message being received in
duplicate, as the QUIC protocol prevents that from happening.

# Finishing channels

When the receiver of a channel finishes a channel, it does so by sending
a FinishSender frame on the channel control stream, then finishes the
channel control stream's channel-sender-to-receiver direction. The
FinishSender frame includes the final reliable and unreliable number of
messages sent on the channel before it finished.

When the receiver receives a FinishSender frame, it enters a "finishing"
state. This state lasts until it finishes finishing, or until something
else happens that rips it out of this state. For the receiver to finish
finishing, it must wait to receive all messages sent on the channel
reliably, and give all messages sent on the channel unreliably a fair
chance to be received. To be more precise about the thing about
unreliable messages, the receiver must wait until either all unreliable
messages sent on the channel have been received, or until a sufficient
delay occurs after the receipt of the FinishSender frame such that the
receiver would've been willing to nack them under logic equivalent to
the logic by which the receiver handles SentUnreliable frames.

Once the receiver has met the above conditions, it sends one final
AckNackUnreliable frame back on the channel control stream, unless such
a frame would be empty of any acks or nacks, and then finishes the
channel control stream's channel-receiver-to-sender direction. After
this, the receiver ceases to exist.

TODO optimization where we only have to deal with acknacking the message
     frames that actually had attachments

# Cancelling channels

When the sender of a channel cancels a channel, it does so by resetting
the channel control stream's channel-sender-to-receiver direction. The
sender should also reset any streams it is using only to send message
frames for that channel, if it still has potentially live handles to any
such streams.

When the receiver observes such a resetting, it performs the "abandon
receiver" procedure.

# Dropping receivers

If the application permanently abandons the taking of messages from the
receiver side of a channel for which the receiver still exists, the
receiver must perform the "abandon receiver" procedure.

# The abandon receiver procedure

The "abandon receiver" procedure is as follows. The receiver sends one
final AckNackUnreliable frame back on the channel control stream, which
must include acks for all unreliable messages frames which been
processed and not yet acked. Any unreliable message frames not yet
processed at this point should be nacked immediately, without waiting
for a delay. This final AckNackUnreliable frame does not have to include
nacks except as necessary to encode acks. The receiver may skip sending
the final AckNackUnreliable frame if it would not contain any acks.
Then, the receiver sends an AbandonReceiver frame on the channel control
stream. The AbandonReceiver frame also contains ack and nack ranges in a
way similar to an AckNackUnreliable frame, except for reliable messages
instead of unreliable messages. All reliable message frames which have
been processed at this point must be acked via the AbandonReceiver
frame, and any not yet received at that point may be nacked, but may be
not nacked except as necessary to encode acks. After this, the receiver
finishes the channel control stream's channel-receiver-to-sender
direction, and then the receiver ceases to exist.

NEEDS WORK elaborate about the race conditions in the above paragraph
           and how overall things here aren't a problem.

The abandon receiver procedure may rip a receiver out of the finishing
state.

TODO cancelling only some messages sent within an unordered stream,
     maybe unreliable too

# Race conditions

This is a non-normative section of the protocol specification that
clarifies how various parts of the system fit together to avoid certain
race conditions.

At a given point in time, both sides of the connection have some set of
senders and receivers. These correspond to channel IDs. A receiver
always has a channel control stream attached to it, whereas a sender
sometimes comes into existence without a channel control stream
attached, and gets one attached later in response to a receiver on the
other end creating one.

Either side of the connection can send messages for any sender it has,
excepting senders in some sort of shutting down state. A message frame
may be lost in transit (despite the connection as a whole remaining
live), either because it was sent on a stream and the stream was
cancelled, or because it was sent on a datagram and the datagram was
lost. The channel control stream involves logic relating to informing
the sender of the exact set of messages that were received or lost.

A message sent is sent on a channel ID. Moreover, a message can contain
a collection of attached senders and receivers, each represented by a
channel ID. The bit in an attached channel ID that indicates which side
minted the channel ID always is the side sending the message with the
attachment, and the bit indicating which direction the channel is
flowing in is used to deduce whether it was the sender or receiver that
was attached to the message.

A sender or receiver can come into existence in these ways:

- The entrypoint sender comes into existence in the client when the
  client first initializes.
- The entrypoint receiver comes into existence in the server when the
  server processes the client's ConnectionControl frame.
- When a side of the connection sends a message with a sender / receiver
  attached to it, it mints a channel ID for the attached sender /
  receiver, and locally creates a corresponding receiver / sender.
- When a side of the connection receives a message with a sender /
  receiver attached to it, it locally creates the corresponding sender /
  receiver, if one does not yet exist.
- When a side of the connection receives a message, it locally creates a
  receiver for the channel ID the message was sent on, if one does not
  yet exist.
- 

# Error handling

Upon encountering or detecting a protocol error, the QUIC connection
should be immediately closed.

If the QUIC connection closes, the Aqueduct connection closes.





























# Glossary

- Aqueduct: This protocol.
- QUIC: The QUIC protocol, as first standardized in RFC 9000, which
  Aqueduct runs on.
- TLS: The encryption protocol which QUIC is fused with.
- ZeroRTT: A feature provided by QUIC and TLS, also known as TLS Early
  Data, or TLS (Stateless) Session Resumption, or 0-RTT.
- Stream: A QUIC byte stream.
- Bidirectional / Unidirectional stream: Streams being bidirectional or
  unidirectional is a feature of QUIC streams, provided by QUIC.
- Finishing a stream: QUIC has a feature where a stream can be
  "finished". This represents a graceful closing of the stream, in which
  all bytes sent will be retransmitted as necessary until the reader
  reads all bytes from the stream, after which the reader observes that
  the stream has finished. In a bidirectional stream, both sides can be
  finished independently. Not to be confused with finishing a channel.
- Resetting a stream: QUIC has a feature where a stream can be "reset".
  This represents an abrupt closing of the stream, in which the reader
  observes that the stream was reset as soon as possible, and any bytes
  buffered, in-transit, or needing of retransmission are abandoned. In
  a bidirectional stream, both sides can be reset independently. The
  analogous concept for channels is cancelling the channel.
- Datagram: A QUIC unreliable datagram, as per the QUIC unreliable
  datagram extension first standardized in RFC 9221.
- Connection: An Aqueduct connection between two sides of the Aqueduct
  connection, which wraps around a QUIC connection.
- Client: The side of the Aqueduct connection which initiated the
  connection.
- Server: The side of the Aqueduct connection which is not the client.
- Channel: A networked channel of messages within an Aqueduct
  connection, which is the main abstraction exposed by Aqueduct to the
  application.
- Oneshot channel: A channel that may only ever have up to 1 message
  sent on it, created like that intentionally by the application.
- Message: A discrete message sent by the application on a channel.
- Payload: The part of a message which is a byte array encoded by
  application-provided serialization logic.
- Attachment: A relationship between a message and a channel, wherein a
  new channel may be created by attaching it to a message that is sent
  on an existing channel.
- Finishing a channel: Aqueduct has a feature where a channel can be
  "finished". This represents a graceful closing of the channel, in
  which all messages sent reliably will be received, and all messages
  sent unreliably will be given a reasonable chance to be received,
  after which the receiver observes that the channel has finished. Not
  to be confused with finishing a stream.
- Cancelling a channel: Aqueduct has a feature where a sender can
  "cancel" a channel. This represents an abrupt closing of the channel,
  in which the receiver observes that the channel was cancelled as soon
  as possible, and any messages not yet received are abandoned. The
  analogous concept for streams is resetting the stream.
- Frame: A unit of Aqueduct protocol communication, written by one side
  of the Aqueduct connection to a stream or datagram and read by the
  other side, without being exposed to the application.