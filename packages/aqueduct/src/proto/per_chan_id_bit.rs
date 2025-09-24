use crate::frame::common::*;
use std::ops::Index;

#[derive(Default)]
pub struct PerCreatorSide<T>(pub [T; 2]);

impl<T> PerCreatorSide<T> {
    pub fn by_creator(&self, creator: Side) -> &T {
        &self.0[creator.0 as usize]
    }
}

impl<T> Index<ChanId> for PerCreatorSide<T> {
    type Output = T;

    fn index(&self, idx: ChanId) -> &Self::Output {
        self.by_creator(idx.creator())
    }
}

#[derive(Default)]
pub struct PerSenderSide<T>(pub [T; 2]);

impl<T> PerSenderSide<T> {
    pub fn by_sender(&self, sender: Side) -> &T {
        &self.0[sender.0 as usize]
    }
}

impl<T> Index<ChanId> for PerSenderSide<T> {
    type Output = T;

    fn index(&self, idx: ChanId) -> &Self::Output {
        self.by_sender(idx.sender())
    }
}

#[derive(Default)]
pub struct PerIsOneshot<T>(pub [T; 2]);

impl<T> PerIsOneshot<T> {
    pub fn by_is_oneshot(&self, is_oneshot: bool) -> &T {
        &self.0[is_oneshot as usize]
    }
}

impl<T> Index<ChanId> for PerIsOneshot<T> {
    type Output = T;

    fn index(&self, idx: ChanId) -> &Self::Output {
        self.by_is_oneshot(idx.is_oneshot())
    }
}
