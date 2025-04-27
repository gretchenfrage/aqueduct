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

1. Zero or one `CONNECTION_HEADERS` frames.
2. Zero or one of the following:
    1. One `ROUTE_TO` frame.
    2. Zero or more frames of types other than `CONNECTION_HEADERS` or
       `ROUTE_TO`.

The `ROUTE_TO` frame contains a chanid that subsequent frames in the sequence
pertain to. The endpoint must read process the subsequent frames sequentially
in the context of its state for that channel.

If a stream is reset, the endpoint ignores any partially received frame.

### 2.2 § Frame types

The following frame types exist, and they are encoded as such:

#### 2.2.1 § `CONNECTION_HEADERS`

1. MAGIC BYTES: The bytes [239, 80, 95, 166, 96, 15, 64, 142].
2. HUMAN TEXT: The ASCII string "AQUEDUCT".
3. VERSION: A varbytes containing the ASCII string "0.0.0-AFTER".
4. CONNECTION_HEADERS: Header data.

#### 2.2.2 § `ROUTE_TO`

1. TAG: The byte 1.
2. CHANNEL: A chanid.

#### 2.2.3 § `MESSAGE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 2.
3. MESSAGE_NUM: A varint.
4. MESSAGE_HEADERS: Header data.
5. ATTACHMENTS: A varbytes containing a sequence of the following:
    1. CHANNEL: A chanid with CREATOR equal to frame sender.
    2. CHANNEL_HEADERS: Header data.
6. PAYLOAD: A varbytes.

#### 2.2.4 § `FINISH_SENDER`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 3.
3. SENT_RELIABLE: A varint.

#### 2.2.5 § `CANCEL_SENDER`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 4.

#### 2.2.6 § `SENT_UNRELIABLE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame sender.

1. TAG: The byte 5.
3. COUNT: A varint.

#### 2.2.7 § `ACK_RELIABLE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame receiver.

1. TAG: The byte 6.
3. RANGES: A varbytes containing a sequence of varints. The number of varints
   is even and non-zero. Every varint except the first must be non-zero.

#### 2.2.8 § `ACK_NACK_UNRELIABLE`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame receiver.

1. TAG: The byte 7.
2. CHANNEL: A chanid with SENDER equal to frame receiver.
3. RANGES: A varbytes containing a sequence of varints. There must be more than
   zero varints. All varints must be nonzero, except the first if there are
   more than one.

#### 2.2.9 § `DROP_RECEIVER`

Contextual: ROUTE_TO.CHANNEL.SENDER must be the frame receiver.

1. TAG: The byte 8.

#### 2.2.10 § `FORGET_CHANNEL`

1. TAG: The byte 9.

## 3 § Protocol operation


