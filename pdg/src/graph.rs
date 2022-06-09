use rustc_index::newtype_index;
use rustc_index::vec::IndexVec;
use rustc_middle::mir::{BasicBlock, Field, Local};
use rustc_span::def_id::DefPathHash;
use c2rust_analysis_rt::{MirPlace};
use std::{collections::HashMap, fmt::Debug};

// Implement `Idx` and other traits like MIR indices (`Local`, `BasicBlock`, etc.)
newtype_index!(
    pub struct GraphId { DEBUG_FORMAT = "GraphId({})" }
);

// Implement `Idx` and other traits like MIR indices (`Local`, `BasicBlock`, etc.)
newtype_index!(
    pub struct NodeId { DEBUG_FORMAT = "NodeId({})" }
);

// Implement `Idx` and other traits like MIR indices (`Local`, `BasicBlock`, etc.)
pub const ROOT_NODE: NodeId = NodeId::from_u32(0);

/// A pointer derivation graph, which tracks the handling of one object throughout its lifetime.
#[derive(Debug)]
pub struct Graph {
    /// The nodes in the graph.  Nodes are stored in increasing order by timestamp.  The first
    /// node, called the "root node", creates the object described by this graph, and all other
    /// nodes are derived from it.
    pub nodes: IndexVec<NodeId, Node>,
}

impl Graph {
    pub fn new() -> Graph {
        Graph {
            nodes: IndexVec::new(),
        }
    }
}

/// A node in the graph represents an operation on pointers.  It may produce a pointer from
/// nothing, derive a pointer from another pointer, or consume a pointer without producing any
/// output.
///
/// Each operation occurs at a point in time, but the timestamp is not stored explicitly.  Instead,
/// nodes in each graph are stored in sequential order, and timing relationships can be identified
/// by comparing `NodeId`s.
#[derive(Debug)]
pub struct Node {
    /// The function that contains this operation.
    ///
    /// For function calls, copies from the caller's values into the callee's argument locals are
    /// attributed to the first statement of the callee, and the copy from the callee's return
    /// place to the caller's destination local is attributed to the `Call` terminator in the
    /// caller.  This way, the combination of `function` and `dest` accurately identifies the local
    /// modified by the operation.
    pub function: DefPathHash,
    /// The basic block that contains this operation.
    pub block: BasicBlock,
    /// The index within the basic block of the MIR statement or terminator that performed this
    /// operation.  As in `rustc_middle::mir::Location`, an index less than the number of
    /// statements in the block refers to that statement, and an index equal to the number of
    /// statements refers to the terminator.
    pub index: usize,
    /// The MIR local where this operation stores its result.  This is `None` for operations that
    /// don't store anything and for operations whose result is a temporary not visible as a MIR
    /// local.
    pub dest: Option<MirPlace>,
    /// The kind of operation that was performed.
    pub kind: NodeKind,
    /// The `Node` that produced the input to this operation.
    pub source: Option<(GraphId, NodeId)>,
}

#[derive(Debug)]
pub enum NodeKind {
    /// A copy from one local to another.  This also covers casts such as `&mut T` to `&T` or `&T`
    /// to `*const T` that don't change the type or value of the pointer.
    Copy,

    /// Field projection.  Used for operations like `_2 = &(*_1).0`.  Nested field accesses like
    /// `_4 = &(*_1).x.y.z` are broken into multiple `Node`s, each covering one level.
    Field(Field),
    /// Pointer arithmetic.  The `isize` is the concrete offset distance.  We use this to detect
    /// when two pointers always refer to different indices.
    Offset(isize),

    // Operations that can't have a `source`.
    /// Get the address of a local.  For address-taken locals, the root node is an `AddrOfLocal`
    /// attributed to the first statement of the function.  Taking the address of the local, as in
    /// `_2 = &_1`, appears as a copy of that root pointer, and reading or writing from the local
    /// shows up as a `LoadAddr` or `StoreAddr`.  This allows us to track uses of the local that
    /// interfere with an existing reference, even when those uses don't go through a pointer.
    AddrOfLocal(Local),
    /// Get the address of a static.  These are treated the same as locals, with an
    /// `AddressOfStatic` attributed to the first statement.
    AddrOfStatic(DefPathHash),
    /// Heap allocation.  The `usize` is the number of array elements allocated; for allocations of
    /// a single object, this value is 1.
    Malloc(usize),
    /// Int to pointer conversion.  Details TBD.
    IntToPtr,
    /// The result of loading a value through some other pointer.  Details TBD.
    LoadValue,

    // Operations that can't be the `source` of any other operation.
    /// Heap deallocation.  The object described by the current graph is no longer valid after this
    /// point.  Correct programs will only `Free` pointers produced by `Malloc`, and will no longer
    /// `LoadAddr` or `StoreAddr` any pointers derived from that `Malloc` afterward.
    Free,
    /// Pointer to int conversion.  Details TBD.
    PtrToInt,
    /// The pointer appears as the address of a load operation.
    LoadAddr,
    /// The pointer appears as the address of a store operation.
    StoreAddr,
    /// The pointer is stored through some other pointer.  Details TBD.
    StoreValue,
}

/// A collection of graphs describing the handling of one or more objects within the program.
pub struct Graphs {
    /// The graphs.  Each graph describes one object, or one group of objects that were all handled
    /// identically.
    pub graphs: IndexVec<GraphId, Graph>,

    /// Lookup table for finding all nodes in all graphs that store to a particular MIR local.
    pub latest_assignment: HashMap<(DefPathHash, usize), (GraphId, NodeId)>,
}

impl Graphs {
    pub fn new() -> Graphs {
        Graphs {
            graphs: IndexVec::new(),
            latest_assignment: HashMap::new(),
        }
    }
}

impl Debug for Graphs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.graphs)
    }
}
