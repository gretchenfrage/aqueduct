use crate::frame::common::*;
use std::ops::Index;

#[derive(Default)]
pub struct PerCreator<T>(pub [T; 2]);

impl<T> PerCreator<T> {
    pub fn by_creator(&self, creator: Side) -> &T {
        &self.0[creator.0 as usize]
    }
}

impl<T> Index<ChanId> for PerCreator<T> {
    type Output = T;

    fn index(&self, idx: ChanId) -> &Self::Output {
        self.by_creator(idx.creator())
    }
}

#[derive(Default)]
pub struct PerSender<T>(pub [T; 2]);

impl<T> PerSender<T> {
    pub fn by_sender(&self, sender: Side) -> &T {
        &self.0[sender.0 as usize]
    }
}

impl<T> Index<ChanId> for PerSender<T> {
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
