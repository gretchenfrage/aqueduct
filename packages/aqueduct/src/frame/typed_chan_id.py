
DIRS = ['', 'LocalSender', 'LocalReceiver']
MINTED_BYS = ['', 'LocallyMinted', 'RemotelyMinted']
IS_ONESHOTS = ['', 'Multishot', 'Oneshot']

def form_wrapper_name(dir, minted_by, is_oneshot):
    return f"ChanId{dir}{minted_by}{is_oneshot}"

print("""//! System for type-checking channel ID bounds.
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
""")

def fn_sort_by_dir(minted_by, is_oneshot):
    local_sender_wrapper_name = form_wrapper_name(DIRS[1], minted_by, is_oneshot)
    local_receiver_wrapper_name = form_wrapper_name(DIRS[2], minted_by, is_oneshot)
    print(f"    /// Categorized this channel ID by its direction relative to the local side.")
    print(f"    pub fn sort_by_dir(self, side: Side) -> SortByDir<{local_sender_wrapper_name}, {local_receiver_wrapper_name}> {{")
    print(f"        if self.dir().side_to() == side {{")
    print(f"            SortByDir::LocalReceiver({local_receiver_wrapper_name}::new_unchecked(self.into()))")
    print(f"        }} else {{")
    print(f"            SortByDir::LocalSender({local_sender_wrapper_name}::new_unchecked(self.into()))")
    print(f"        }}")
    print(f"    }}")

def fn_sort_by_minted_by(dir, is_oneshot):
    locally_minted_wrapper_name = form_wrapper_name(dir, MINTED_BYS[1], is_oneshot)
    remotely_minted_wrapper_name = form_wrapper_name(dir, MINTED_BYS[2], is_oneshot)
    print(f"    /// Categorized this channel ID by which side minted it relative to the local side.")
    print(f"    pub fn sort_by_minted_by(self, side: Side) -> SortByMintedBy<{locally_minted_wrapper_name}, {remotely_minted_wrapper_name}> {{")
    print(f"        if self.minted_by() == side {{")
    print(f"            SortByMintedBy::LocallyMinted({locally_minted_wrapper_name}::new_unchecked(self.into()))")
    print(f"        }} else {{")
    print(f"            SortByMintedBy::RemotelyMinted({remotely_minted_wrapper_name}::new_unchecked(self.into()))")
    print(f"        }}")
    print(f"    }}")

def fn_sort_by_is_oneshot(dir, minted_by):
    multishot_wrapper_name = form_wrapper_name(dir, minted_by, IS_ONESHOTS[1])
    oneshot_wrapper_name = form_wrapper_name(dir, minted_by, IS_ONESHOTS[2])
    print(f"    /// Categorized this channel ID by whether it is multishot.")
    print(f"    pub fn sort_by_is_oneshot(self) -> SortByIsOneshot<{multishot_wrapper_name}, {oneshot_wrapper_name}> {{")
    print(f"        if self.is_oneshot() {{")
    print(f"            SortByIsOneshot::Oneshot({oneshot_wrapper_name}::new_unchecked(self.into()))")
    print(f"        }} else {{")
    print(f"            SortByIsOneshot::Multishot({multishot_wrapper_name}::new_unchecked(self.into()))")
    print(f"        }}")
    print(f"    }}")

print("impl ChanId {")
fn_sort_by_dir('', '')
print()
fn_sort_by_minted_by('', '')
print()
fn_sort_by_is_oneshot('', '')
print("}")

def ty_chan_id(dir, minted_by, is_oneshot):
    if dir == '' and minted_by == '' and is_oneshot == '':
        return

    wrapper_name = form_wrapper_name(dir, minted_by, is_oneshot)

    print()
    print(f"/// Newtype wrapper around chan id.")
    print(f"#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]")
    print(f"pub struct {wrapper_name}(ChanId);")
    print()
    print(f"impl {wrapper_name} {{")
    print(f"    /// Construct without checking component constraints.")
    print(f"    pub fn new_unchecked(chan_id: ChanId) -> Self {{")
    print(f"        {wrapper_name}(chan_id)")
    print(f"    }}")
    print()
    if dir == '':
        print(f"    /// Get direction part.")
        print(f"    pub fn dir(self) -> Dir {{")
        print(f"         self.0.dir()")
        print(f"    }}")
        print()
        fn_sort_by_dir(minted_by, is_oneshot)
        print()
    if minted_by == '':
        print(f"    /// Get minted-by part.")
        print(f"    pub fn minted_by(self) -> Side {{")
        print(f"        self.0.minted_by()")
        print(f"    }}")
        print()
        fn_sort_by_minted_by(dir, is_oneshot)
        print()
    if is_oneshot == '':
        print(f"    /// Get is-oneshot part.")
        print(f"    pub fn is_oneshot(self) -> bool {{")
        print(f"        self.0.is_oneshot()")
        print(f"    }}")
        print()
        fn_sort_by_is_oneshot(dir, minted_by)
        print()
    print(f"    /// Get channel index part.")
    print(f"    pub fn idx(self) -> u64 {{")
    print(f"        self.0.idx()")
    print(f"    }}")
    print(f"}}")
    print()
    print(f"impl From<{wrapper_name}> for ChanId {{")
    print(f"    fn from(subtype: {wrapper_name}) -> ChanId {{")
    print(f"        subtype.0")
    print(f"    }}")
    print(f"}}")
    print()

    for dir_2 in DIRS:
        for minted_by_2 in MINTED_BYS:
            for is_oneshot_2 in IS_ONESHOTS:
                ty_chan_id_into(dir, minted_by, is_oneshot, dir_2, minted_by_2, is_oneshot_2)

def ty_chan_id_into(dir_1, minted_by_1, is_oneshot_1, dir_2, minted_by_2, is_oneshot_2):
    if dir_2 == '' and minted_by_2 == '' and is_oneshot_2 == '':
        return
    if [dir_1, minted_by_1, is_oneshot_1] == [dir_2, minted_by_2, is_oneshot_2]:
        return
    if dir_2 != dir_1 and dir_2 != '':
        return
    if minted_by_2 != minted_by_1 and minted_by_2 != '':
        return
    if is_oneshot_2 != is_oneshot_1 and is_oneshot_2 != '':
        return

    wrapper_name_1 = form_wrapper_name(dir_1, minted_by_1, is_oneshot_1)
    wrapper_name_2 = form_wrapper_name(dir_2, minted_by_2, is_oneshot_2)

    print(f"impl From<{wrapper_name_1}> for {wrapper_name_2} {{")
    print(f"    fn from(subtype: {wrapper_name_1}) -> Self {{")
    print(f"        {wrapper_name_2}::new_unchecked(subtype.0)")
    print(f"    }}")
    print(f"}}")
    print()

for dir in DIRS:
    for minted_by in MINTED_BYS:
        for is_oneshot in IS_ONESHOTS:
            ty_chan_id(dir, minted_by, is_oneshot)

print("""
/// Typed version of the entrypoint channel constant from the cilent perspective.
pub const CLIENT_ENTRYPOINT: ChanIdLocalSenderLocallyMintedMultishot = ChanIdLocalSenderLocallyMintedMultishot(ChanId::ENTRYPOINT);

/// Typed version of the entrypoint channel constant from the cilent perspective.
pub const SERVER_ENTRYPOINT: ChanIdLocalReceiverRemotelyMintedMultishot = ChanIdLocalReceiverRemotelyMintedMultishot(ChanId::ENTRYPOINT);
""")
