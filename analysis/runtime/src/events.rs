use serde::{Deserialize, Serialize};
use std::fmt;
use crate::mir_loc::{self, MirLocId};

pub type Pointer = usize;

#[derive(Serialize,Deserialize)]
pub struct Event {
    pub mir_loc: MirLocId,
    pub kind: EventKind,
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(mir_loc) = mir_loc::get(self.mir_loc) {
            mir_loc.fmt(f)?;
        } else {
            self.mir_loc.fmt(f)?;
        }
        write!(f, " {:?}", self.kind)
    }
}

impl Event {
    pub fn done() -> Self {
        Self {
            mir_loc: 0,
            kind: EventKind::Done,
        }
    }
}

#[derive(Serialize,Deserialize,Copy,Clone)]
pub enum EventKind {
    /// A copy from one local to another. This also covers casts such as `&mut
    /// T` to `&T` or `&T` to `*const T` that don't change the type or value of
    /// the pointer.
    Copy(Pointer),

    /// Field projection. Used for operations like `_2 = &(*_1).0`. Nested field
    /// accesses like `_4 = &(*_1).x.y.z` are broken into multiple `Node`s, each
    /// covering one level.
    Field(Pointer, u32),

    Alloc {
        size: usize,
        ptr: Pointer,
    },
    Free {
        ptr: Pointer,
    },
    Realloc {
        old_ptr: Pointer,
        size: usize,
        new_ptr: Pointer,
    },
    Arg(Pointer),
    Ret(Pointer),
    Done,

    /// The pointer appears as the address of a load operation.
    LoadAddr(Pointer),

    /// The pointer appears as the address of a store operation.
    StoreAddr(Pointer),
}

impl fmt::Debug for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EventKind::Copy(ptr) => write!(f, "copy({:p})", ptr as *const u8),
            EventKind::Field(ptr, id) => write!(f, "field({:p}, {})", ptr as *const u8, id),
            EventKind::Alloc { size, ptr } => {
                write!(f, "malloc({}) -> {:p}", size, ptr as *const u8)
            }
            EventKind::Free { ptr } => write!(f, "free({:p})", ptr as *const u8),
            EventKind::Realloc { old_ptr, size, new_ptr } => write!(
                f,
                "realloc({:p}, {}) -> {:p}",
                old_ptr as *const u8, size, new_ptr as *const u8
            ),
            EventKind::Arg(ptr) => write!(f, "arg({:p})", ptr as *const u8),
            EventKind::Ret(ptr) => write!(f, "ret({:p})", ptr as *const u8),
            EventKind::Done => write!(f, "done"),
            EventKind::LoadAddr(ptr) => write!(f, "load({:p})", ptr as *const u8),
            EventKind::StoreAddr(ptr) => write!(f, "store({:p})", ptr as *const u8),
        }
    }
}
