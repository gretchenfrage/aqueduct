<img align="right" height="75" src="docs/.assets/aqueduct.png"/>

# The Aqueduct Protocol: Reference Specification

Note: It is recommended to read [`OVERVIEW.md`](OVERVIEW.md) first.

## 1 § Encoding low-level primitives

### 1.1 § Endianness

All values are encoded little-endian.

### 1.2 § Varint

A variable length unsigned integer is encoded as a sequence of 1 or more bytes,
where the lowest 7 bits of each byte encode the next lowest 7 bits of the
integer, and the highest bit of each byte is 1 if there are additional bytes in
the sequence.

### 1.3 § Varbytes

A variable length sequence of bytes is encoded as a varint conveying the number
of bytes, followed by the bytes.

### 1.4 § Varranges

A variable length sequence of unsigned integer ranges is encoded as a varbytes
containing a sequence of varints. The varints alternate between the length of
the next range and the length of the gap before the next range. All varints
except the first and last must be non-zero.

### 1.5 § Header data

Header data is encoded as a varbytes containing a sequence of multiple inner
varbytes. The number of inner varbytes is always a multiple of 2. Each pair of
2 inner varbytes represents a key/value pair.

### 1.6 § Chanid

A channel ID uniquely identified a channel within a connection. It is a bit
field encoded as a varint. Its bits, from lowest to highest, are:

1. CREATOR (1 bit): 0 if the client created the channel, 1 if the server
   created the channel.
2. SENDER (1 bit): 0 if the client owns the sender half, 1 if the server owns
   the sender half.
3. ONESHOT (1 bit): 0 if the channel is multishot, 1 if the channel is oneshot.
4. INDEX (61 bits): An unsigned integer, assigned sequentially within the space
   defined by the other 3 bits, via an integer counter owned by CREATOR.

Naturally, the entrypoint chanid is all zeroes. Thus, the index counter for
that index space starts at 1. All other index counters start at 0.

## 2 § Frames

### 2.1 § Receiving frames

Aqueduct utilizes QUIC unidirectional streams and QUIC unreliable datagrams,
both of which contain a sequence of frames. An endpoint must read and process
these frames. A frame sequence always follows the pattern:

1. Zero or more of the following:
    1. Any of the following frame types: `VERSION`, `ACK_VERSION`,
       `CONNECTION_HEADERS`.
2. Zero or one of the following:
    1. One `ROUTE_TO` frame.
    2. Zero or more frames of types other than the following: `VERSION`,
       `ACK_VERSION`, `CONNECTION_HEADERS`, `ROUTE_TO`.

The `ROUTE_TO` frame contains a chanid that subsequent frames in the sequence
pertain to. The endpoint must read process the subsequent frames sequentially
in the context of its state for that channel.

If a stream is reset, the endpoint ignores any partially received frame.

### 2.2 § Frame types

The following frame types exist, and they are encoded as such:

#### 2.2.1 § `VERSION`

1. MAGIC BYTES: The bytes [239, 80, 95, 166, 96, 15, 64, 142].
2. HUMAN TEXT: The ASCII string "AQUEDUCT".
3. VERSION: A varbytes containing the ASCII string "0.0.0-AFTER".

#### 2.2.2 § `ACK_VERSION`

1. TAG: The byte 1.

#### 2.2.3 § `CONNECTION_HEADERS`

1. TAG: The byte 2.
2. CONNECTION_HEADERS: Header data.

#### 2.2.4 § `ROUTE_TO`

1. TAG: The byte 3.
2. CHANNEL: A chanid.

#### 2.2.5 § `MESSAGE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 4.
3. MESSAGE_NUM: A varint.
4. MESSAGE_HEADERS: Header data.
5. ATTACHMENTS: A varbytes containing a sequence of the following:
    1. CHANNEL: A chanid with CREATOR equal to frame sender.
    2. CHANNEL_HEADERS: Header data.
6. PAYLOAD: A varbytes.

#### 2.2.6 § `FINISH_SENDER`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 5.
3. SENT_RELIABLE: A varint.

#### 2.2.7 § `CANCEL_SENDER`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 6.

#### 2.2.8 § `SENT_UNRELIABLE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 7.
3. COUNT: A varint.

#### 2.2.9 § `ACK_RELIABLE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame receiver.

1. TAG: The byte 8.
3. RANGES: A varbytes containing a sequence of varints. The number of varints
   is even and non-zero. Every varint except the first must be non-zero.

#### 2.2.10 § `ACK_NACK_UNRELIABLE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame receiver.

1. TAG: The byte 9.
2. CHANNEL: A chanid with SENDER equal to frame receiver.
3. RANGES: A varbytes containing a sequence of varints. There must be more than
   zero varints. All varints must be nonzero, except the first if there are
   more than one.

#### 2.2.11 § `DROP_RECEIVER`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame receiver.

1. TAG: The byte 10.

#### 2.2.12 § `FORGET_CHANNEL`

1. TAG: The byte 11.

## 3 § Protocol operation

Both sides of the connection maintain a set of sender state machines and
receiver state machines, each associated with a channel ID for which the
endpoint is the sender / receiver. When the connection initializes, there is
only a single sender state machine on the client and receiver state machine on
the server, for the entrypoint channel.

### 3.1 § Version negotiation

Both sides must begin all frame sequences with a `VERSION` frame until they
receive an `ACK_VERSION` frame. When an endpoint first receives a `VERSION`
frame it must send an `ACK_VERSION` frame in a stream. If an endpoint receives
a frame sequence that does not begin with a `VERSION` frame before it has sent
an `ACK_VERSION` frame it must close the connection without processing
subsequent frames.

### 3.2 § Connection headers

When the connection starts, the client must send the server a
`CONNECTION_HEADERS` frame in a stream, containing the client's connection
headers. When the server receives this, it must send back a
`CONNECTION_HEADERS` frame in a stream, containing the server's connection
headers. Each side must wait to process any frames other than `VERSION` frames
until it has received a `CONNECTION_HEADERS` frame. Each side must only send
a `CONNECTION_HEADERS` frame once.


