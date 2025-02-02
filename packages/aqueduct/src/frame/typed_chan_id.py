
DIRS = ['E', 'S', 'R']
MINTED_BYS = ['e', 'l', 'r']
IS_ONESHOTS = ['e', 'm', 'o']

def form_wrapper_name(dir, minted_by, is_oneshot):
    return f"ChanId{dir}{minted_by}{is_oneshot}"

print("//! System for type-checking channel ID bounds.")
print("//!")
print("//! It follows the convention of `ChanId` followed by 3 letters, representing 3 axes:")
print("//!")
print("//! 1. Direction: `E` for either, `S` for sender, `R` for receiver.")
print("//! 2. Minted By: `E` for either, `L` for local, `R` for remote.")
print("//! 3. Is Oneshot: `E` for either, `M` for multishot, `O` for oneshot.")
print("//!")
print("//! This entire module is procedurally generated from a Python script and checked into git, via the")
print("//! following procedure:")
print("//!")
print("//! 1. `cd` into the parent directory")
print("//! 2. Run `python3 typed_chan_id.py > typed_chan_id.rs`")
print()
print("use crate::frame::common::{ChanId, Dir, Side};")
print()

def ty_chan_id(dir, minted_by, is_oneshot):
    if dir == 'E' and minted_by == 'e' and is_oneshot == 'e':
        return

    print()
    print(f"/// Newtype wrapper around chan id.")
    print(f"///")

    if dir == 'E':
        print("/// - It may be flowing in either direction")
    elif dir == 'S':
        print("/// - The local side is the sender")
    elif dir == 'R':
        print("/// - The local side is the receiver")
    else:
        raise ""

    if minted_by == 'e':
        print("/// - It may have been minted by either side")
    elif minted_by == 'l':
        print("/// - It was minted by the local side")
    elif minted_by == 'r':
        print("/// - It was minted by the remote side")
    else:
        raise ""

    if is_oneshot == 'e':
        print("/// - It may be either oneshot or multishot")
    elif is_oneshot == 'm':
        print("/// - It is multishot")
    elif is_oneshot == 'o':
        print("/// - It is oneshot")
    else:
        raise ""

    wrapper_name = form_wrapper_name(dir, minted_by, is_oneshot)

    print(f"#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]")
    print(f"pub struct {wrapper_name}(ChanId);")
    print()
    print(f"impl {wrapper_name} {{")
    print(f"    /// Construct without checking component constraints.")
    print(f"    pub fn new_unchecked(chan_id: ChanId) -> Self {{")
    print(f"        {wrapper_name}(chan_id)")
    print(f"    }}")
    print()
    if dir == 'E':
        print(f"    /// Get direction part.")
        print(f"    pub fn dir(self) -> Dir {{")
        print(f"         self.0.dir()")
        print(f"    }}")
        print()
    if minted_by != 'e':
        print(f"    /// Get minted-by part.")
        print(f"    pub fn minted_by(self) -> Side {{")
        print(f"        self.0.minted_by()")
        print(f"    }}")
        print()
    if is_oneshot == 'e':
        print(f"    /// Get is-oneshot part.")
        print(f"    pub fn is_oneshot(self) -> bool {{")
        print(f"        self.0.is_oneshot()")
        print(f"    }}")
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
    if dir_2 == 'E' and minted_by_2 == 'e' and is_oneshot_2 == 'e':
        return
    if [dir_1, minted_by_1, is_oneshot_1] == [dir_2, minted_by_2, is_oneshot_2]:
        return
    if dir_2 != dir_1 and dir_2 != 'E':
        return
    if minted_by_2 != minted_by_1 and minted_by_2 != 'e':
        return
    if is_oneshot_2 != is_oneshot_1 and is_oneshot_2 != 'e':
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
