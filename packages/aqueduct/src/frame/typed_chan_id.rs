//! System for type-checking channel ID bounds.
//!
//! This entire module is procedurally generated from a Python script and checked into git, via the
//! following procedure:
//!
//! 1. `cd` into the parent directory
//! 2. Run `python3 typed_chan_id.py > typed_chan_id.rs`

use crate::frame::common::{ChanId, Dir, Side};


/// Channel ID categorized by its direction relative to the local side.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SortByDir<A, B> {
    LocalSender(A),
    LocalReceiver(B),
}

/// Channel ID categorized by which side minted it relative to the local side.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SortByMintedBy<A, B> {
    LocallyMinted(A),
    RemotelyMinted(B),
}

/// Channel ID categorized by whether it is multishot.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub enum SortByIsOneshot<A, B> {
    Multishot(A),
    Oneshot(B),
}

impl ChanId {
    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSender, ChanIdLocalReceiver> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiver::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSender::new_unchecked(self.into()))
        }
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocallyMinted, ChanIdRemotelyMinted> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocallyMinted::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdRemotelyMinted::new_unchecked(self.into()))
        }
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdMultishot, ChanIdOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdMultishot::new_unchecked(self.into()))
        }
    }
}

/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdMultishot(ChanId);

impl ChanIdMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdMultishot(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderMultishot, ChanIdLocalReceiverMultishot> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverMultishot::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderMultishot::new_unchecked(self.into()))
        }
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocallyMintedMultishot, ChanIdRemotelyMintedMultishot> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocallyMintedMultishot::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdMultishot> for ChanId {
    fn from(subtype: ChanIdMultishot) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdOneshot(ChanId);

impl ChanIdOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdOneshot(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderOneshot, ChanIdLocalReceiverOneshot> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverOneshot::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderOneshot::new_unchecked(self.into()))
        }
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocallyMintedOneshot, ChanIdRemotelyMintedOneshot> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdRemotelyMintedOneshot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdOneshot> for ChanId {
    fn from(subtype: ChanIdOneshot) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocallyMinted(ChanId);

impl ChanIdLocallyMinted {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocallyMinted(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderLocallyMinted, ChanIdLocalReceiverLocallyMinted> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverLocallyMinted::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderLocallyMinted::new_unchecked(self.into()))
        }
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocallyMintedMultishot, ChanIdLocallyMintedOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocallyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocallyMinted> for ChanId {
    fn from(subtype: ChanIdLocallyMinted) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocallyMintedMultishot(ChanId);

impl ChanIdLocallyMintedMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocallyMintedMultishot(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderLocallyMintedMultishot, ChanIdLocalReceiverLocallyMintedMultishot> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverLocallyMintedMultishot::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderLocallyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocallyMintedMultishot> for ChanId {
    fn from(subtype: ChanIdLocallyMintedMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocallyMintedMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocallyMintedMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocallyMintedMultishot> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocallyMintedMultishot) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocallyMintedOneshot(ChanId);

impl ChanIdLocallyMintedOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocallyMintedOneshot(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderLocallyMintedOneshot, ChanIdLocalReceiverLocallyMintedOneshot> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderLocallyMintedOneshot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocallyMintedOneshot> for ChanId {
    fn from(subtype: ChanIdLocallyMintedOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocallyMintedOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocallyMintedOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocallyMintedOneshot> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocallyMintedOneshot) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRemotelyMinted(ChanId);

impl ChanIdRemotelyMinted {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRemotelyMinted(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderRemotelyMinted, ChanIdLocalReceiverRemotelyMinted> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverRemotelyMinted::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderRemotelyMinted::new_unchecked(self.into()))
        }
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdRemotelyMintedMultishot, ChanIdRemotelyMintedOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdRemotelyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRemotelyMinted> for ChanId {
    fn from(subtype: ChanIdRemotelyMinted) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRemotelyMintedMultishot(ChanId);

impl ChanIdRemotelyMintedMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRemotelyMintedMultishot(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderRemotelyMintedMultishot, ChanIdLocalReceiverRemotelyMintedMultishot> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverRemotelyMintedMultishot::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRemotelyMintedMultishot> for ChanId {
    fn from(subtype: ChanIdRemotelyMintedMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRemotelyMintedMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdRemotelyMintedMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRemotelyMintedMultishot> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdRemotelyMintedMultishot) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRemotelyMintedOneshot(ChanId);

impl ChanIdRemotelyMintedOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRemotelyMintedOneshot(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Categorized this channel ID by its direction relative to the local side.
    pub fn sort_by_dir(self, side: Side) -> SortByDir<ChanIdLocalSenderRemotelyMintedOneshot, ChanIdLocalReceiverRemotelyMintedOneshot> {
        if self.dir().side_to() == side {
            SortByDir::LocalReceiver(ChanIdLocalReceiverRemotelyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByDir::LocalSender(ChanIdLocalSenderRemotelyMintedOneshot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRemotelyMintedOneshot> for ChanId {
    fn from(subtype: ChanIdRemotelyMintedOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRemotelyMintedOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdRemotelyMintedOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRemotelyMintedOneshot> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdRemotelyMintedOneshot) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSender(ChanId);

impl ChanIdLocalSender {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSender(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocalSenderLocallyMinted, ChanIdLocalSenderRemotelyMinted> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocalSenderLocallyMinted::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdLocalSenderRemotelyMinted::new_unchecked(self.into()))
        }
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocalSenderMultishot, ChanIdLocalSenderOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocalSenderOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocalSenderMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSender> for ChanId {
    fn from(subtype: ChanIdLocalSender) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderMultishot(ChanId);

impl ChanIdLocalSenderMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderMultishot(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocalSenderLocallyMintedMultishot, ChanIdLocalSenderRemotelyMintedMultishot> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocalSenderLocallyMintedMultishot::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdLocalSenderRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderMultishot> for ChanId {
    fn from(subtype: ChanIdLocalSenderMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocalSenderMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderMultishot> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderMultishot) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderOneshot(ChanId);

impl ChanIdLocalSenderOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderOneshot(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocalSenderLocallyMintedOneshot, ChanIdLocalSenderRemotelyMintedOneshot> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocalSenderLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdLocalSenderRemotelyMintedOneshot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderOneshot> for ChanId {
    fn from(subtype: ChanIdLocalSenderOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocalSenderOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderOneshot> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderOneshot) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderLocallyMinted(ChanId);

impl ChanIdLocalSenderLocallyMinted {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderLocallyMinted(chan_id)
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocalSenderLocallyMintedMultishot, ChanIdLocalSenderLocallyMintedOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocalSenderLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocalSenderLocallyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderLocallyMinted> for ChanId {
    fn from(subtype: ChanIdLocalSenderLocallyMinted) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderLocallyMinted> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocalSenderLocallyMinted) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMinted> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderLocallyMinted) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderLocallyMintedMultishot(ChanId);

impl ChanIdLocalSenderLocallyMintedMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderLocallyMintedMultishot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanId {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanIdLocallyMintedMultishot {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> Self {
        ChanIdLocallyMintedMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanIdLocalSenderMultishot {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> Self {
        ChanIdLocalSenderMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedMultishot> for ChanIdLocalSenderLocallyMinted {
    fn from(subtype: ChanIdLocalSenderLocallyMintedMultishot) -> Self {
        ChanIdLocalSenderLocallyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderLocallyMintedOneshot(ChanId);

impl ChanIdLocalSenderLocallyMintedOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderLocallyMintedOneshot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanId {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanIdLocallyMintedOneshot {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> Self {
        ChanIdLocallyMintedOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanIdLocalSenderOneshot {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> Self {
        ChanIdLocalSenderOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderLocallyMintedOneshot> for ChanIdLocalSenderLocallyMinted {
    fn from(subtype: ChanIdLocalSenderLocallyMintedOneshot) -> Self {
        ChanIdLocalSenderLocallyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderRemotelyMinted(ChanId);

impl ChanIdLocalSenderRemotelyMinted {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderRemotelyMinted(chan_id)
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocalSenderRemotelyMintedMultishot, ChanIdLocalSenderRemotelyMintedOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocalSenderRemotelyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocalSenderRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderRemotelyMinted> for ChanId {
    fn from(subtype: ChanIdLocalSenderRemotelyMinted) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderRemotelyMinted> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdLocalSenderRemotelyMinted) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMinted> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderRemotelyMinted) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderRemotelyMintedMultishot(ChanId);

impl ChanIdLocalSenderRemotelyMintedMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderRemotelyMintedMultishot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanId {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanIdRemotelyMintedMultishot {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> Self {
        ChanIdRemotelyMintedMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanIdLocalSenderMultishot {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> Self {
        ChanIdLocalSenderMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedMultishot> for ChanIdLocalSenderRemotelyMinted {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedMultishot) -> Self {
        ChanIdLocalSenderRemotelyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalSenderRemotelyMintedOneshot(ChanId);

impl ChanIdLocalSenderRemotelyMintedOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalSenderRemotelyMintedOneshot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanId {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanIdRemotelyMintedOneshot {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> Self {
        ChanIdRemotelyMintedOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanIdLocalSender {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> Self {
        ChanIdLocalSender::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanIdLocalSenderOneshot {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> Self {
        ChanIdLocalSenderOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalSenderRemotelyMintedOneshot> for ChanIdLocalSenderRemotelyMinted {
    fn from(subtype: ChanIdLocalSenderRemotelyMintedOneshot) -> Self {
        ChanIdLocalSenderRemotelyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiver(ChanId);

impl ChanIdLocalReceiver {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiver(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocalReceiverLocallyMinted, ChanIdLocalReceiverRemotelyMinted> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocalReceiverLocallyMinted::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdLocalReceiverRemotelyMinted::new_unchecked(self.into()))
        }
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocalReceiverMultishot, ChanIdLocalReceiverOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocalReceiverOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocalReceiverMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiver> for ChanId {
    fn from(subtype: ChanIdLocalReceiver) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverMultishot(ChanId);

impl ChanIdLocalReceiverMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverMultishot(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocalReceiverLocallyMintedMultishot, ChanIdLocalReceiverRemotelyMintedMultishot> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocalReceiverLocallyMintedMultishot::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdLocalReceiverRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverMultishot> for ChanId {
    fn from(subtype: ChanIdLocalReceiverMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocalReceiverMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverMultishot> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverMultishot) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverOneshot(ChanId);

impl ChanIdLocalReceiverOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverOneshot(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Categorized this channel ID by which side minted it relative to the local side.
    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<ChanIdLocalReceiverLocallyMintedOneshot, ChanIdLocalReceiverRemotelyMintedOneshot> {
        if self.minted_by() == side {
            SortByMintedBy::LocallyMinted(ChanIdLocalReceiverLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByMintedBy::RemotelyMinted(ChanIdLocalReceiverRemotelyMintedOneshot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverOneshot> for ChanId {
    fn from(subtype: ChanIdLocalReceiverOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocalReceiverOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverOneshot> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverOneshot) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverLocallyMinted(ChanId);

impl ChanIdLocalReceiverLocallyMinted {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverLocallyMinted(chan_id)
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocalReceiverLocallyMintedMultishot, ChanIdLocalReceiverLocallyMintedOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocalReceiverLocallyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocalReceiverLocallyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverLocallyMinted> for ChanId {
    fn from(subtype: ChanIdLocalReceiverLocallyMinted) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverLocallyMinted> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocalReceiverLocallyMinted) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMinted> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverLocallyMinted) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverLocallyMintedMultishot(ChanId);

impl ChanIdLocalReceiverLocallyMintedMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverLocallyMintedMultishot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanId {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanIdLocallyMintedMultishot {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> Self {
        ChanIdLocallyMintedMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanIdLocalReceiverMultishot {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> Self {
        ChanIdLocalReceiverMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedMultishot> for ChanIdLocalReceiverLocallyMinted {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedMultishot) -> Self {
        ChanIdLocalReceiverLocallyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverLocallyMintedOneshot(ChanId);

impl ChanIdLocalReceiverLocallyMintedOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverLocallyMintedOneshot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanId {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanIdLocallyMinted {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> Self {
        ChanIdLocallyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanIdLocallyMintedOneshot {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> Self {
        ChanIdLocallyMintedOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanIdLocalReceiverOneshot {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> Self {
        ChanIdLocalReceiverOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverLocallyMintedOneshot> for ChanIdLocalReceiverLocallyMinted {
    fn from(subtype: ChanIdLocalReceiverLocallyMintedOneshot) -> Self {
        ChanIdLocalReceiverLocallyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverRemotelyMinted(ChanId);

impl ChanIdLocalReceiverRemotelyMinted {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverRemotelyMinted(chan_id)
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Categorized this channel ID by whether it is multishot.
    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<ChanIdLocalReceiverRemotelyMintedMultishot, ChanIdLocalReceiverRemotelyMintedOneshot> {
        if self.is_oneshot() {
            SortByIsOneshot::Oneshot(ChanIdLocalReceiverRemotelyMintedOneshot::new_unchecked(self.into()))
        } else {
            SortByIsOneshot::Multishot(ChanIdLocalReceiverRemotelyMintedMultishot::new_unchecked(self.into()))
        }
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverRemotelyMinted> for ChanId {
    fn from(subtype: ChanIdLocalReceiverRemotelyMinted) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverRemotelyMinted> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdLocalReceiverRemotelyMinted) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMinted> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverRemotelyMinted) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverRemotelyMintedMultishot(ChanId);

impl ChanIdLocalReceiverRemotelyMintedMultishot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverRemotelyMintedMultishot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanId {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanIdMultishot {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> Self {
        ChanIdMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanIdRemotelyMintedMultishot {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> Self {
        ChanIdRemotelyMintedMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanIdLocalReceiverMultishot {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> Self {
        ChanIdLocalReceiverMultishot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedMultishot> for ChanIdLocalReceiverRemotelyMinted {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedMultishot) -> Self {
        ChanIdLocalReceiverRemotelyMinted::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdLocalReceiverRemotelyMintedOneshot(ChanId);

impl ChanIdLocalReceiverRemotelyMintedOneshot {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdLocalReceiverRemotelyMintedOneshot(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanId {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanIdOneshot {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> Self {
        ChanIdOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanIdRemotelyMinted {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> Self {
        ChanIdRemotelyMinted::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanIdRemotelyMintedOneshot {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> Self {
        ChanIdRemotelyMintedOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanIdLocalReceiver {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> Self {
        ChanIdLocalReceiver::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanIdLocalReceiverOneshot {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> Self {
        ChanIdLocalReceiverOneshot::new_unchecked(subtype.0)
    }
}

impl From<ChanIdLocalReceiverRemotelyMintedOneshot> for ChanIdLocalReceiverRemotelyMinted {
    fn from(subtype: ChanIdLocalReceiverRemotelyMintedOneshot) -> Self {
        ChanIdLocalReceiverRemotelyMinted::new_unchecked(subtype.0)
    }
}


/// Typed version of the entrypoint channel constant from the cilent perspective.
pub const CLIENT_ENTRYPOINT: ChanIdLocalSenderLocallyMintedMultishot = ChanIdLocalSenderLocallyMintedMultishot(ChanId::ENTRYPOINT);

/// Typed version of the entrypoint channel constant from the cilent perspective.
pub const SERVER_ENTRYPOINT: ChanIdLocalReceiverRemotelyMintedMultishot = ChanIdLocalReceiverRemotelyMintedMultishot(ChanId::ENTRYPOINT);

