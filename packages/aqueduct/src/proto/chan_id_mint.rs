
use crate::frame::common::*;
use std::sync::atomic::{
    AtomicU64,
    Ordering::Relaxed,
};


/// Utility for minting local channel IDs concurrently with atomics.
pub struct ChanIdMint {
    side: Side,
    next_idxs: [[AtomicU64; 2]; 2],
}

impl ChanIdMint {
    pub fn new(side: Side) -> Self {
        let mut next_idxs = <[[AtomicU64; 2]; 2]>::default();
        if side == Side::Client {
            // entrypoint channel id
            next_idxs[Dir::ToServer as usize][false as usize] = 1.into();
        }
        ChanIdMint { side, next_idxs }
    }

    fn mint(&self, dir: Dir, is_oneshot: bool) -> ChanId {
        let idx = self.next_idxs[dir as usize][is_oneshot as usize].fetch_add(1, Relaxed);
        ChanId::new(dir, self.side, is_oneshot, idx)
    }

    pub fn mint_local_sender(&self) -> ChanIdLocalSenderLocallyMintedMultishot {
        let chan_id = self.mint(self.side.opposite().dir_to(), false);
        ChanIdLocalSenderLocallyMintedMultishot::new_unchecked(chan_id)
    }

    pub fn mint_local_receiver(&self) -> ChanIdLocalReceiverLocallyMintedMultishot {
        let chan_id = self.mint(self.side.dir_to(), false);
        ChanIdLocalReceiverLocallyMintedMultishot::new_unchecked(chan_id)
    }

    pub fn mint_local_sender_oneshot(&self) -> ChanIdLocalSenderLocallyMintedOneshot {
        let chan_id = self.mint(self.side.opposite().dir_to(), true);
        ChanIdLocalSenderLocallyMintedOneshot::new_unchecked(chan_id)
    }

    pub fn mint_local_receiver_oneshot(&self) -> ChanIdLocalReceiverLocallyMintedOneshot {
        let chan_id = self.mint(self.side.dir_to(), true);
        ChanIdLocalReceiverLocallyMintedOneshot::new_unchecked(chan_id)
    }
}
