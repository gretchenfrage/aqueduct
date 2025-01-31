// implementation of the aqueduct channel.
//
// the basic architecture is vaguely inspired by flume. it is as such:
//
// channel handles wrap around Arc<Mutex<shared state>>
//                                         |
//          /------------------------------/
//          v
//       shared state
//          |
//          |------ it contains a seg_queue::SegQueue<T>, which is an externally-safe,
//          |       not-itself-concurrent data structure used to hold the buffered elements
//          |
//          |------ it contains a "send node queue":
//          |
//          |       this is a linked queue of nodes each of which correspondings to a pending send
//          |       future. the corresponding send future has a pointer directly to its node,
//          |       although dereferencing that pointer is still guarded by the central mutex. the
//          |       node allocation has a slot for storing a Waker, and the fact that they form a
//          |       queue structure is used to achieve fairness.
//          |
//          \------ it contains a "recv node queue", which is the same idea for recv futures.
//
// there are also some additional things like atomic variables for send and recv count, variables
// for tracking bounds, etc.
//
// blocking versions of operations are built as a layer on top of the futures, in a low-cost way
// in the polling module that does some scary tricks with memory.
//
// the organization of these modules is as such:
//
//      These are used like
//      library utilities:
//    /--------------------\
//
//      seg_queue<-------------core: This is the sin-eater of the unsafety. It presents an
//                   |         ^     abstraction for channels which is fully safe and sound, but
//      node_queue<--/         |     panicky and inconvenient.
//                             |
//      polling<---------------api: This is a wrapper around core that adapts it into an API that
//                                  is convenient and defensive. The crate re-exports this API
//                                  publically.
//
// there is also the error module, which contains the relevant error types, which is also
// re-exported publically.

pub(crate) mod error;
pub(crate) mod api;

mod seg_queue;
mod node_queue;
mod polling;
mod core;
