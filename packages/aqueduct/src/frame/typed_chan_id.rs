//! System for type-checking channel ID bounds.
//!
//! It follows the convention of `ChanId` followed by 3 letters, representing 3 axes:
//!
//! 1. Direction: `E` for either, `S` for sender, `R` for receiver.
//! 2. Minted By: `E` for either, `L` for local, `R` for remote.
//! 3. Is Oneshot: `E` for either, `M` for multishot, `O` for oneshot.
//!
//! This entire module is procedurally generated from a Python script and checked into git, via the
//! following procedure:
//!
//! 1. `cd` into the parent directory
//! 2. Run `python3 typed_chan_id.py > typed_chan_id.rs`

use crate::frame::common::{ChanId, Dir, Side};


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It may have been minted by either side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdEee(ChanId);

impl ChanIdEee {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdEee(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdEee> for ChanId {
    fn from(subtype: ChanIdEee) -> ChanId {
        subtype.0
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It may have been minted by either side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdEem(ChanId);

impl ChanIdEem {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdEem(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdEem> for ChanId {
    fn from(subtype: ChanIdEem) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdEem> for ChanIdEee {
    fn from(subtype: ChanIdEem) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It may have been minted by either side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdEeo(ChanId);

impl ChanIdEeo {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdEeo(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdEeo> for ChanId {
    fn from(subtype: ChanIdEeo) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdEeo> for ChanIdEee {
    fn from(subtype: ChanIdEeo) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It was minted by the local side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdEle(ChanId);

impl ChanIdEle {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdEle(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdEle> for ChanId {
    fn from(subtype: ChanIdEle) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdEle> for ChanIdEee {
    fn from(subtype: ChanIdEle) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It was minted by the local side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdElm(ChanId);

impl ChanIdElm {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdElm(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdElm> for ChanId {
    fn from(subtype: ChanIdElm) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdElm> for ChanIdEee {
    fn from(subtype: ChanIdElm) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdElm> for ChanIdEem {
    fn from(subtype: ChanIdElm) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdElm> for ChanIdEle {
    fn from(subtype: ChanIdElm) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It was minted by the local side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdElo(ChanId);

impl ChanIdElo {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdElo(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdElo> for ChanId {
    fn from(subtype: ChanIdElo) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdElo> for ChanIdEee {
    fn from(subtype: ChanIdElo) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdElo> for ChanIdEeo {
    fn from(subtype: ChanIdElo) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdElo> for ChanIdEle {
    fn from(subtype: ChanIdElo) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It was minted by the remote side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdEre(ChanId);

impl ChanIdEre {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdEre(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdEre> for ChanId {
    fn from(subtype: ChanIdEre) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdEre> for ChanIdEee {
    fn from(subtype: ChanIdEre) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It was minted by the remote side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdErm(ChanId);

impl ChanIdErm {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdErm(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdErm> for ChanId {
    fn from(subtype: ChanIdErm) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdErm> for ChanIdEee {
    fn from(subtype: ChanIdErm) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdErm> for ChanIdEem {
    fn from(subtype: ChanIdErm) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdErm> for ChanIdEre {
    fn from(subtype: ChanIdErm) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - It may be flowing in either direction
/// - It was minted by the remote side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdEro(ChanId);

impl ChanIdEro {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdEro(chan_id)
    }

    /// Get direction part.
    pub fn dir(self) -> Dir {
         self.0.dir()
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdEro> for ChanId {
    fn from(subtype: ChanIdEro) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdEro> for ChanIdEee {
    fn from(subtype: ChanIdEro) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdEro> for ChanIdEeo {
    fn from(subtype: ChanIdEro) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdEro> for ChanIdEre {
    fn from(subtype: ChanIdEro) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It may have been minted by either side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSee(ChanId);

impl ChanIdSee {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSee(chan_id)
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSee> for ChanId {
    fn from(subtype: ChanIdSee) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSee> for ChanIdEee {
    fn from(subtype: ChanIdSee) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It may have been minted by either side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSem(ChanId);

impl ChanIdSem {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSem(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSem> for ChanId {
    fn from(subtype: ChanIdSem) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSem> for ChanIdEee {
    fn from(subtype: ChanIdSem) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSem> for ChanIdEem {
    fn from(subtype: ChanIdSem) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSem> for ChanIdSee {
    fn from(subtype: ChanIdSem) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It may have been minted by either side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSeo(ChanId);

impl ChanIdSeo {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSeo(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSeo> for ChanId {
    fn from(subtype: ChanIdSeo) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSeo> for ChanIdEee {
    fn from(subtype: ChanIdSeo) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSeo> for ChanIdEeo {
    fn from(subtype: ChanIdSeo) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSeo> for ChanIdSee {
    fn from(subtype: ChanIdSeo) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It was minted by the local side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSle(ChanId);

impl ChanIdSle {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSle(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSle> for ChanId {
    fn from(subtype: ChanIdSle) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSle> for ChanIdEee {
    fn from(subtype: ChanIdSle) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSle> for ChanIdEle {
    fn from(subtype: ChanIdSle) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSle> for ChanIdSee {
    fn from(subtype: ChanIdSle) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It was minted by the local side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSlm(ChanId);

impl ChanIdSlm {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSlm(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSlm> for ChanId {
    fn from(subtype: ChanIdSlm) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSlm> for ChanIdEee {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlm> for ChanIdEem {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlm> for ChanIdEle {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlm> for ChanIdElm {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdElm::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlm> for ChanIdSee {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlm> for ChanIdSem {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdSem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlm> for ChanIdSle {
    fn from(subtype: ChanIdSlm) -> Self {
        ChanIdSle::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It was minted by the local side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSlo(ChanId);

impl ChanIdSlo {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSlo(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSlo> for ChanId {
    fn from(subtype: ChanIdSlo) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSlo> for ChanIdEee {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlo> for ChanIdEeo {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlo> for ChanIdEle {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlo> for ChanIdElo {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdElo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlo> for ChanIdSee {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlo> for ChanIdSeo {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdSeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSlo> for ChanIdSle {
    fn from(subtype: ChanIdSlo) -> Self {
        ChanIdSle::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It was minted by the remote side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSre(ChanId);

impl ChanIdSre {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSre(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSre> for ChanId {
    fn from(subtype: ChanIdSre) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSre> for ChanIdEee {
    fn from(subtype: ChanIdSre) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSre> for ChanIdEre {
    fn from(subtype: ChanIdSre) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSre> for ChanIdSee {
    fn from(subtype: ChanIdSre) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It was minted by the remote side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSrm(ChanId);

impl ChanIdSrm {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSrm(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSrm> for ChanId {
    fn from(subtype: ChanIdSrm) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSrm> for ChanIdEee {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSrm> for ChanIdEem {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSrm> for ChanIdEre {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSrm> for ChanIdErm {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdErm::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSrm> for ChanIdSee {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSrm> for ChanIdSem {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdSem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSrm> for ChanIdSre {
    fn from(subtype: ChanIdSrm) -> Self {
        ChanIdSre::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the sender
/// - It was minted by the remote side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdSro(ChanId);

impl ChanIdSro {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdSro(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdSro> for ChanId {
    fn from(subtype: ChanIdSro) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdSro> for ChanIdEee {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSro> for ChanIdEeo {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSro> for ChanIdEre {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSro> for ChanIdEro {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdEro::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSro> for ChanIdSee {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdSee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSro> for ChanIdSeo {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdSeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdSro> for ChanIdSre {
    fn from(subtype: ChanIdSro) -> Self {
        ChanIdSre::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It may have been minted by either side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRee(ChanId);

impl ChanIdRee {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRee(chan_id)
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRee> for ChanId {
    fn from(subtype: ChanIdRee) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRee> for ChanIdEee {
    fn from(subtype: ChanIdRee) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It may have been minted by either side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRem(ChanId);

impl ChanIdRem {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRem(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRem> for ChanId {
    fn from(subtype: ChanIdRem) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRem> for ChanIdEee {
    fn from(subtype: ChanIdRem) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRem> for ChanIdEem {
    fn from(subtype: ChanIdRem) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRem> for ChanIdRee {
    fn from(subtype: ChanIdRem) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It may have been minted by either side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdReo(ChanId);

impl ChanIdReo {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdReo(chan_id)
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdReo> for ChanId {
    fn from(subtype: ChanIdReo) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdReo> for ChanIdEee {
    fn from(subtype: ChanIdReo) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdReo> for ChanIdEeo {
    fn from(subtype: ChanIdReo) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdReo> for ChanIdRee {
    fn from(subtype: ChanIdReo) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It was minted by the local side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRle(ChanId);

impl ChanIdRle {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRle(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRle> for ChanId {
    fn from(subtype: ChanIdRle) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRle> for ChanIdEee {
    fn from(subtype: ChanIdRle) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRle> for ChanIdEle {
    fn from(subtype: ChanIdRle) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRle> for ChanIdRee {
    fn from(subtype: ChanIdRle) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It was minted by the local side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRlm(ChanId);

impl ChanIdRlm {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRlm(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRlm> for ChanId {
    fn from(subtype: ChanIdRlm) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRlm> for ChanIdEee {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlm> for ChanIdEem {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlm> for ChanIdEle {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlm> for ChanIdElm {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdElm::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlm> for ChanIdRee {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlm> for ChanIdRem {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdRem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlm> for ChanIdRle {
    fn from(subtype: ChanIdRlm) -> Self {
        ChanIdRle::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It was minted by the local side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRlo(ChanId);

impl ChanIdRlo {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRlo(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRlo> for ChanId {
    fn from(subtype: ChanIdRlo) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRlo> for ChanIdEee {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlo> for ChanIdEeo {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlo> for ChanIdEle {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdEle::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlo> for ChanIdElo {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdElo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlo> for ChanIdRee {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlo> for ChanIdReo {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdReo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRlo> for ChanIdRle {
    fn from(subtype: ChanIdRlo) -> Self {
        ChanIdRle::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It was minted by the remote side
/// - It may be either oneshot or multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRre(ChanId);

impl ChanIdRre {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRre(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get is-oneshot part.
    pub fn is_oneshot(self) -> bool {
        self.0.is_oneshot()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRre> for ChanId {
    fn from(subtype: ChanIdRre) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRre> for ChanIdEee {
    fn from(subtype: ChanIdRre) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRre> for ChanIdEre {
    fn from(subtype: ChanIdRre) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRre> for ChanIdRee {
    fn from(subtype: ChanIdRre) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It was minted by the remote side
/// - It is multishot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRrm(ChanId);

impl ChanIdRrm {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRrm(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRrm> for ChanId {
    fn from(subtype: ChanIdRrm) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRrm> for ChanIdEee {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRrm> for ChanIdEem {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdEem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRrm> for ChanIdEre {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRrm> for ChanIdErm {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdErm::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRrm> for ChanIdRee {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRrm> for ChanIdRem {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdRem::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRrm> for ChanIdRre {
    fn from(subtype: ChanIdRrm) -> Self {
        ChanIdRre::new_unchecked(subtype.0)
    }
}


/// Newtype wrapper around chan id.
///
/// - The local side is the receiver
/// - It was minted by the remote side
/// - It is oneshot
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ChanIdRro(ChanId);

impl ChanIdRro {
    /// Construct without checking component constraints.
    pub fn new_unchecked(chan_id: ChanId) -> Self {
        ChanIdRro(chan_id)
    }

    /// Get minted-by part.
    pub fn minted_by(self) -> Side {
        self.0.minted_by()
    }

    /// Get channel index part.
    pub fn idx(self) -> u64 {
        self.0.idx()
    }
}

impl From<ChanIdRro> for ChanId {
    fn from(subtype: ChanIdRro) -> ChanId {
        subtype.0
    }
}

impl From<ChanIdRro> for ChanIdEee {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdEee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRro> for ChanIdEeo {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdEeo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRro> for ChanIdEre {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdEre::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRro> for ChanIdEro {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdEro::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRro> for ChanIdRee {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdRee::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRro> for ChanIdReo {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdReo::new_unchecked(subtype.0)
    }
}

impl From<ChanIdRro> for ChanIdRre {
    fn from(subtype: ChanIdRro) -> Self {
        ChanIdRre::new_unchecked(subtype.0)
    }
}

