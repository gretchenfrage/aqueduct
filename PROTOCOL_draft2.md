# The Aqueduct Protocol

Aqueduct is a network protocol based on the central idea of sending channels
within channels, built on top of QUIC.

## Introduction

The most important object that an Aqueduct implementation provides is a
"channel". A channel is a queue of messages with sender and receiver handles
that can be used simultaneously by multiple parallel tasks to coordinate. This
should be a concept that is familiar to many engineers.

```pseudocode
    sender, receiver = new_channel<Int>()
    spawn_thread:
        while message = receiver.receive():
            print(message)
    for i in [0, 10]:
        sleep(1 second)
        sender.send(i)
```

A channel can be local, meaning that the senders and receivers are in the same
process. Alternatively, a channel can be networked, such that messages sent by
the sender half are encoded and transmitted over an Aqueduct connection to the
process with the receiver half. Code built over Aqueduct can be largely
oblivious to whether a given channel is local or networked.

```txt
      computer 1:                                 computer 2:
    +--------------------+                      +--------------------+
    | sender => receiver | Aqueduct connection: | sender => receiver |
    |                    |----------------------|                    |
    | sender ====================================> receiver          |
    | receiver <================================== sender            |
    |                    |----------------------|                    |
    +--------------------+                      +--------------------+
```

The above notions are useful, but not particularly novel--the key feature of
Aqueduct lies in how a networked channel is created. Almost all channels are
initially created as local channels, by calling a constructor that returns a
linked sender/receiver pair. Then, by sending the sender half or the receiver
half down a different pre-existing networked channel, the new channel
automatically and seamlessly becomes networked, within the same Aqueduct
connection as the pre-existing channel.

This allows code built over Aqueduct to be oblivious not only to whether it is
using networked or local channels, but also to whether it is _creating_
networked or local channels. This property allows application developers to
create powerful, asynchronous, and network-optimized abstractions that
cross-cut network boundaries.

```txt
    1. pre-existing networked channel

    +------------------------+   +--------------+
    | sender 1 ====================> receiver 1 |
    |                        |   |              |
    |                        |   |              |
    +------------------------+   +--------------+

    2. create a new channel

    +------------------------+   +--------------+
    | sender 1 ====================> receiver 1 |
    |                        |   |              |
    | sender 2 => receiver 2 |   |              |
    +------------------------+   +--------------+

    3. send receiver 2 down sender 1

    +------------------------+   +--------------+
    | sender 1 ====================> receiver 1 |
    |                        |   |              |
    | sender 2 ====================> receiver 2 |
    +------------------------+   +--------------+

    3. (possibility 2) send sender 2 down sender 1

    +------------------------+   +--------------+
    | sender 1 ====================> receiver 1 |
    |                        |   |              |
    | receiver 2 <================== sender 2   |
    +------------------------+   +--------------+
```

```pseudocode
    struct MyMessage:
        value: Int
        sender_2: Sender<int>

    function left_half(sender_1: Sender<MyMessage>):
        sender_2, receiver_2 = new_channel<MyMessage>()
        sender_1.send(MyMessage(42, sender_2))
        response_value = receiver_2.receive()
        print(response_value)

    function right_half(receiver_1: Sender<MyMessage>)
        my_message = receiver_1.receive()
        my_message.sender_2.send(my_message.value * 2)


    ^-- Hint: This program is equivalent to making an RPC call to multiply the number 7 by 2.

        Because of Aqueduct, left_half and right_half are oblivious to whether they are running in
        the same process / whether their channels are networked or local. They simply have to be
        called with a linked sender/receiver pair in parallel.
```

The one channel not created in this way is the "entrypoint" channel--the first
networked channel constructed by a client connecting to a server, from which
all other networked channels are bootstrapped.

```pseudocode
                             +--- Note: That's an IPV6 address.
                             |
    function run_server:     v
        receiver = bind<Int>([::]:8080)
        while message = receiver.receive():
            print(message)

    function run_client:
        sender = connect<Int>([::1]:8080)
        sender.send(42)
        sender.send(666)


    In run_server, bind creates the entrypoint receiver, which any client can
    create a networked sender handle to. In run_client, connect establishes an
    Aqueduct connection to the server and returns its entrypoint sender, which
    is connected to the server's receiver.
```


