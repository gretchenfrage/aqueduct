
# Aqueduct's plan to ZeroRTT everything

We'll start by building an intuition for some fundamental constraints on
latency in encrypted messaging. Imagine this abstract model of a basic
encrypted messaging protocol:

           CLIENT     SERVER
      0-RTT    ()
                 \
    0.5-RTT       \->()

The client wants to send the server an encrypted message, and the client
already knows the server's public key. This is an "asynchronous" protocol,
meaning that there is no back-and-forth; the client could conceivably die
before the server starts up, with an intermediary holding the message in the 
mean time.

Consider what happens if an attacker makes a copy of the message, and then
sends a duplicate copy of the message to the server afterwards. Under many
protocol designs, this would result in the server processing the message a
second time.

There are three possible ways the system could deal with this:

1. **ALLOW REPLAYS**: Simply accept the fact that a message being transmitted
   once means that attackers could cause it to be processed many more times,
   and somehow ensure that this doesn't actually cause problems anyways.

2. **SERVER STATE**: Have the server store local state by which it can track
   the fact that a particular transmission has been processed already, and
   refuse to process it again if a duplicate is received.

3. **HANDSHAKE**: Make the protocol not an asynchronous protocol, but rather,
   one where the client and the server must perform additional round trips of
   communication to establish a connection session before the client can
   actually transmit its payload.

TLS primarily uses the handshake approach. The latency profile of a typical TLS
1.3 handshake looks like this:

           CLIENT    SERVER
      0-RTT   (1)
                 \
    0.5-RTT       \->(2)
                    /
      1-RTT   (3)<-/       <== Client gains ability to send application data
                 \
    1.5-RTT       \->(4)   <== Server gains ability to send application data

Both the client and the server gain the ability to send secured application
data once they have gotten a chance to send a handshake message and receive
back a response for it: the client at 1-RTT, and the server at 1.5-RTT. Thus,
if the client creates a TLS connection and sends, say, an RPC request as soon
as it is able (1-RTT), the server may send back the RPC response at 1.5-RTT,
and the client would receive the response at 2-RTT. Overall, the TLS handshake
adds 1 RTT latency to the cold-start RPC call.

TLS 1.3's makes it possible to bypass this latency overhead by allowing both
the client and the server to send "early data":

- **0-RTT**: If the client remembers what keys the server was using from some
  previous time it connected to the server, as well as certain transport
  parameters such as the receive buffer size, it may send 0-RTT data, which
  gets transmitted beginnning at 0-RTT, rather than at 1-RTT. However, this
  data is vulnerable to replay attacks.

- **0.5-RTT**: Even without remembering any data from the client, the server
  may send what is sometimes referred to as 0.5-RTT data, because it is sent
  0.5 RTT after the client begins the handshake rather than 1.5 RTT after. This
  is less risky than when the client does it--it is not vulnerable to replay
  attacks. However, if TLS mutual authentication is being used (where the
  server uses TLS to authenticate the client in addition to the other way
  around), 0.5-RTT data is vulnerable to being sent to a client without the
  client having been authenticated, as client authentication occurs at 1.5-RTT.

Thus, sending a request in 0-RTT data and sending the response in 0.5-RTT may
shave off 1 RTT of cold-start latency. In TLS-over-TCP, the RPC call takes 2
RTT instead of 3; in QUIC (which has TLS fused in), the RPC call takes 1 RTT
instead of 2.

Despite these benefits, utilization of Early Data still seems to be rare. I
would venture to hypothesize that this may have to do with the fact that
intentionally opening your application up to replay attacks is terrifying. Some
engineering standards have tried to recommend utilizing Early Data in https GET
requests, since GET requests are supposed to be idempotent. However, consider
this excerpt from [Verified Models and Reference Implementations for the TLS
1.3 Standard Candidate (K Bhargavan et al., 2017)][1]

> Instead of preventing replays, TLS 1.3 Draft-18 advises applications that
> they should only send non-forward-secret and idempotent data over 0-RTT. This
> recommendation is hard to systematically enforce in flexible protocols like
> HTTPS, where all requests have secret cookies attached, and even GET requests
> routinely change state.
>
> We argue that replays offer an important attack vector for 0-RTT and 0.5-RTT
> data. If the client authenticates its 0-RTT flight, then an attacker can
> replay the entire flight to mount authenticated replay attacks. Suppose the
> (client-authenticated) 0-RTT data asks the server to send a client's bank
> statement, and the server sends this data in a 0.5-RTT response. An attacker
> who observes the 0-RTT request once, can replay it any number of times to the
> server from anywhere in the world and the server will send it the user's
> (encrypted) bank statement. Although the attacker cannot complete the 1-RTT
> handshake or read this 0.5-RTT response, it may be able to learn a lot from
> this exchange, such as the length of the bank statement, and whether the
> client is logged in.
>
> In response to these concerns, client authentication has now been removed
> from 0-RTT. However, we note that similar replay attacks apply to 0-RTT data
> that contains an authentication cookie or OAuth token. We highly recommend
> that TLS 1.3 servers should implement a replay cache (based on the client
> nonce nC and the ticket age) to detect and reject replayed 0-RTT data. This
> is less practical in server farms, where time-based replay mitigation may be
> the only alternative.

[1]: https://ieeexplore.ieee.org/stamp/stamp.jsp?tp=&arnumber=7958594

TODO finish this document
