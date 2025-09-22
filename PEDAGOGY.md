<img align="right" height="75" src=".assets/aqueduct.png"/>

# The Aqueduct Protocol: Pedagogical Explantion of Protocol Design

This is a non-normative document intended to give an intuitive understanding of
the Aqueduct protocol’s architecture.

An Aqueduct connection runs over a QUIC connection, between the client and the
server. Other than a few exceptions, the protocol is symmetrical between client
and server.

Aqueduct is primarily concerned with maintaining corresponding state machines
for linked sender/receiver pairs on opposite sides of the connection. An
Aqueduct connection ultimately is a distributed system, albeit one that
ontologically only has 2 nodes. Typically within a distributed system a lot of
the complexity comes from the fact that messages may not be processed by the
thing processing them in a predictable order relative to each other due to them
being processed by entirely different nodes, and also because it may be
possible for some messages to be lost entirely without the entire system
fail-stopping. However, due to the power of the QUIC protocol, we have
engineered ways to bring these problems down to the humble 2-node system as
well!

It is conventional to for an Aqueduct implementation to allow the user to
construct a channel in an initially non-networked state, and then make the
channel networked by sending one side of it within a message on another
networked channel. However, this is purely an API nicety, and not something the
protocol itself is aware of. From the perspective of the protocol
implementation, the connection begins with solely the entrypoint channel
existing, and additional channels are created by sending a message on a
pre-existing channel with the newly created channel attached to it. This
naturally leads to the channels within a connection forming a tree structure of
which channel was used to create another channel, wherein the root of the tree
is the entrypoint channel, and all other channels have a unique lineage of
parent channels linking them back to the entrypoint. It is notable that the
nodes of this tree can flip back and forth in terms of which side of the
connection created them and which direction the channel’s messages
flow—however, a channel’s creator-side will always equal the sender-side of its
parent channel.

Channels within an Aqueduct connection are uniquely identified by channel IDs.
A channel ID is a bit field consisting of 3 boolean flags and a 61-bit
sequential integer ID. The 3 boolean flags indicate whether the channel was
created by the client or by the server, whether the channel is flowing to the
server or to the client, and whether the channel is multishot or oneshot. The
sequential integer ID is allocated sequentially by a counter unique to the
combination of the other 3 bits and owned by the side of the connection that is
creating the channel. The channel ID consisting entirely of zeroes is known as
the entrypoint channel, and is treated specially in certain ways.

Over the wire, the Aqueduct protocol is based on the two sides sending frames
to each other over both QUIC unidirectional streams and QUIC unreliable
datagrams. The most central frame type is the MESSAGE frame, which the sender
side of a channel sends to convey a message being sent on that channel. A
MESSAGE frame morally contains the channel ID it is being sent on, the message
payload (a byte string), and a list of additional channel IDs that are attached
to the message (corresponding to a collection of additional channels that get
newly created by the sending of this message).

- If a sender is operating in ORDERED mode, all of its MESSAGE frames are sent
  on the same QUIC stream.
- If a sender is operating in UNORDERED mode, each of its MESSAGE frames are
  sent on different QUIC streams.
- If a sender is operating in UNRELIABLE mode, each of its MESSAGE frames are
  sent on QUIC unreliable datagrams, unless they are too big to fit in an
  unreliable datagram, in which case Aqueduct falls back to sending it in a
  QUIC stream.

MESSAGE frames are morally the only frame type that Aqueduct sometimes sends in
unreliable datagrams. There are a variety of other frame types dedicated to
maintaining and the lifecycle of channels, the connection, and other important
things; and these are only sent on QUIC streams.

This document will guide the reader to an intuitive understanding of the design
of the Aqueduct protocol in the following way:

- First, we will imagine how the Aqueduct protocol could be designed if we
  assumed all frames would be delivered reliably and in the correct order
  (basically, if we were constructing Aqueduct on top of a TCP connection), and
  also if we assumed that neither side of the connection ever forgets
  information (so, we allow both sides’ memory consumption to grow unboundedly).
- Next, we will introduce relaxations to our assumptions regarding ordering and
  reliability, and discuss how the Aqueduct protocol handles the various
  possible race conditions and other complications caused by that.
- Finally, we will introduce rules for when the Aqueduct implementation is
  allowed to delete certain state and reclaim its memory, discuss what possible
  race conditions and complications that can cause (especially in conjunction
  with message reordering and loss), and discuss how the Aqueduct protocol
  handles that as well.

Imagine first the simplest case of a client which creates the connection, sends
some messages to the server on the entrypoint channel, and then finishes the
channel:

|frames client sends to server|
|-----------------------------|
|msg                          |
|msg                          |
|msg                          |
|finish                       |

The server would simply enqueue these messages into a buffer for the
server-side application to dequeue. Once the server receives the frame
indicating that the channel is finished, it similarly allows its application to
observe this fact.

In the real protocol, there may be race conditions between MESSAGES frames and
a FINISH frame on a channel. For example, if the sender is in UNORDERED mode
then each MESSAGE frame will be sent in a different QUIC stream, meaning that
there is no one QUIC stream the FINISH frame can be sent on to ensure it's
processed after all MESSAGE frames. However, this can be easily fixed by having
the FINISH frame contain the total number of messages sent before finishing,
and if the receiver receives it before receiving that many messages it simply
marks down that the stream finishes after that many messages, and waits until
that number of messages have been received before actually considering the
channel finished. On the other hand, a finishing a sender that is in UNRELIABLE
mode creates some entirely new complications.

There are basically 3 ways the Aqueduct API can be used to intentionally close
a channel:

- By the sender-side finishing it.
- By the sender-side cancelling it.
- By the receiver-side dropping the receiver.

In the above example execution, if the client closed this channel by cancelling
it rather than by finishing it, this simplified protocol execution could be
quite similar. Rather than sending a FINISH frame, the client could send a
CANCEL frame. The main difference would be that upon the server receving this
frame it would drop any of the messages it enqueued in its buffer that the
server-side application hadn't yet dequeued:

|frames client sends to server|
|-----------------------------|
|msg                          |
|msg                          |
|msg                          |
|cancel                       |

In the case that the server-side application closed the channel by dropping its
receiver, the server could drop any enqueued messages and send back a frame to
the client telling it that the receiver has been dropped. Upon receiving this,
the client would allow its application to observe this fact:

|frames client sends to server|frames server sends to client|
|-----------------------------|-----------------------------|
|msg                          |drop receiver                |
|msg                          |                             |
|msg                          |                             |

It's worth noting that, in both such cases, the client doesn't inherently know
how many of the messages it sent the server-side application dequeued before
dropping the rest and entering a state of ignoring additional ones. Moreover,
in the receiver-dropping case, the client sending message frames and even
possibly even a finish or cancel frame can occur in parallel to the server
sending a drop receiver frame, such that both occur before their transmitting
side has received the frames from the other.

Imagine, then, that the client creates an additional client-to-server channel
by attaching it to one of the entrypoint channel messages, and sends messages
on the second channel as well:

|frames client sends to server    |
|---------------------------------|
|chan 1 msg                       |
|chan 1 msg                       |
|chan 1 msg (attachments=[chan 2])|
|chan 2 msg                       |
|chan 2 msg                       |
|chan 1 msg                       |
|chan 1 finish                    |
|chan 2 msg                       |
|chan 2 finish                    |

When the server receives chan 1 msg 2, it knows to create a receiver state
machine for chan 2, since it was attached. After that point, the server can
process messages pertaining to chan 2. In this simplified protocol, the
relative ordering between frames pertaining to chan 1 and chan 2 doesn't matter
except that messages pertaining to chan 2 must occur after the chan 1 frame
which creates chan 2.

In the real protocol, on the other hand, there may be race conditions between
the receipt of the message that creates chan 2 and the receipt of frames
pertaining to chan 2. Since they are different channels, their respective
frames will be sent on different QUIC streams, meaning that one side may start
receiving frames pertaining to a remotely created channel before it learns the
context in which that channel was created. This is handled by simply implicitly
creating the state machine for a channel upon first receiving a frame addressed
to it.

Things can get tricky when we combine these two concepts of channel creation
and channel cancelling (or other things that cause messages never to be
dequeued). Imagine that chan 1 was used to create chan 2, but then chan 1 was
cancelled:

|frames client sends to server    |
|---------------------------------|
|chan 1 msg                       |
|chan 1 msg                       |
|chan 1 msg (attachments=[chan 2])|
|chan 2 msg                       |
|chan 2 msg                       |
|chan 1 msg                       |
|chan 1 cancel                    |
|chan 2 msg                       |
|chan 2 finish                    |

The implications of this situation depend largely on whether the server-side
application dequeued the chan 1 msg containing chan 2 before the server
received the chan 2 cancel frame:

- If so, then the server-side application would experience dequeueing 3 or 4
  messages from channel 1, the 3rd of which would contain the receiver for
  channel 2, before experiencing channel 1 being cancelled--and it would also
  experience receiving 3 messages from channel 2 followed by channel 2
  finishing.
- If not, then the server-side application would experience dequeueing between
  0 and 3 messages from channel 1, followed by channel 1 being cancelled. The
  server-side application would _not_ experience anything about channel 2
  whatsoever, since, even if the server (protocol implementation) received and
  buffered messages for channel 2 and the fact that channel 2 finished, there
  would be no API calls the server-side application could reasonably be
  expected to perform to observe and handle this, since the server-side
  application never received the handle to channel 2 in the first place.

Similar situations could occur due to other reasons than the sender cancelling
the channel as well. The receiver dropping is one example. Another notable one
is if the sender is operating in UNRELIABLE mode, which creates the possibility
that any individual MESSAGE frame will not merely be actively rejected by the
receiver but that it will simply never arrive.

Moreover, there is an element of transitivity to this situation. Consider the
following simplified protocol execution:

|frames client sends to server    |
|---------------------------------|
|chan 1 msg (attachments=[chan 2])|
|chan 2 msg (attachments=[chan 3])|
|chan 2 finish                    |
|chan 3 msg                       |
|chan 3 finish                    |
|chan 1 cancel                    |

Thinking back to how channels naturally form a tree of which channels were used
to create them, channel 1 was used to create channel 2 which was used to create
channel 3. Since channel 1 was cancelled, channel 2 may or may not be
inaccessible to the server-side application. If it is, this necessarily means
that channel 3 is also inaccessible to the server-side application, since the
application would never be able to dequeue the channel 3 receiver from channel
2 if it is never able to dequeue the channel 2 receiver from channel 1.

Thus, we arrive at the notion of loss in transit detection. The principle here
is that, given the possibility of situations where the application on one side
of the connection has a handle to a channel but where the application on the
other side of the connection will never have a handle to that channel, the
Aqueduct protocol automatically detects this situation and ensures that:

- For the side that does have the handles, Aqueduct surfaces "LostInTransit"
  errors so that the application can properly clean up application-level state.
- For the side that will never have the handles, Aqueduct ensures that the
  protocol implementation will be able to garbage-collect any in-memory state
  it has accumulated pertaining to this channel.

At a high level, Aqueduct's strategy for achieving this is as such: For a given
channel, as the sender side sends MESSAGE frames, the receiver side sends back:

- Acks, for message numbers it has received and processed.
- Committing nacks, wherein it not only certifies that is has never received a
  certain message number, but also commits to rejecting and ignoring that
  message number if it happens to receive it in the future.

For senders in ORDERED or UNORDERED mode, nacks don't come into play until the
channel is closed via sender-cancelling or receiver-dropping. For senders in
UNRELIABLE mode, however, the sender pairs unreliable transmission of MESSAGE
frames with reliable transmission of metadata about which message numbers it
has unreliably transmitted, so that in the event these messages don't arrive
within a reasonable time bound the receiver knows to nack them and record state
to ensures it rejects them if they're simply arriving late.

The sender-side of the channel, on the other hand, tracks information about
which additional channels it has created by sending messages on the current
one. When it receives a nack for any such message, it knows that any such
additional channel is lost in transit, as it all of its descendants, and
handles this by garbage-collecting its local state for these channels,
surfacing a LostInTransit error to its local application, and transmitting
LOST_IN_TRANSIT frames for those channels to the remote side so that it knows
to garbage-collect its state for them.

The reason why the receiver side transmits back acks, in addition to nacks, is
just so that the sender side can garbage-collect the necessary tracking state
for the above mechanism once it is known that it is no longer necessary.


