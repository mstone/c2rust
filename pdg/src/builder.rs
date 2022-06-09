use crate::graph::{Graph, GraphId, Graphs, Node, NodeId, NodeKind};
use bincode;
use c2rust_analysis_rt::events::{Event, EventKind};
use c2rust_analysis_rt::mir_loc::{EventMetadata, Metadata};
use c2rust_analysis_rt::{mir_loc, MirLoc, MirPlace};
use log;
use rustc_data_structures::fingerprint::Fingerprint;
use rustc_hir::def_id::DefPathHash;
use rustc_middle::mir::{Field, Local};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;

pub fn read_event_log(path: String) -> Vec<Event> {
    let file = File::open(path).unwrap();
    let mut events = vec![];
    let mut reader = BufReader::new(file);
    loop {
        match bincode::deserialize_from(&mut reader) {
            Ok(e) => events.push(e),
            _ => break,
        }
    }
    events
}

pub fn read_metadata(path: String) -> Metadata {
    let file = File::open(path).unwrap();
    bincode::deserialize_from(file).unwrap()
}

/** return the ptr of interest for a particular event */
fn get_ptr(kind: &EventKind, metadata: &EventMetadata) -> Option<usize> {
    Some(match kind {
        EventKind::CopyPtr(lhs) => *lhs,
        EventKind::Field(ptr, ..) => *ptr,
        EventKind::Free { ptr } => *ptr,
        EventKind::Ret(ptr) => *ptr,

        EventKind::LoadAddr(ptr) => *ptr,
        EventKind::StoreAddr(ptr) => *ptr,
        EventKind::LoadValue(ptr) => *ptr,
        EventKind::StoreValue(ptr) => *ptr,
        EventKind::CopyRef => return None, // FIXME
        EventKind::ToInt(ptr) => *ptr,
        EventKind::Realloc { old_ptr, .. } => *old_ptr,
        EventKind::FromInt(lhs) => *lhs,
        EventKind::Alloc { ptr, .. } => *ptr,
        EventKind::AddrOfLocal(lhs, _) => *lhs,
        EventKind::Offset(ptr, _, _) => *ptr,
        EventKind::Done => return None,
    })
}

fn get_parent_object(kind: &EventKind, obj: (GraphId, NodeId)) -> Option<(GraphId, NodeId)> {
    Some(match kind {
        EventKind::Realloc { new_ptr, .. } => return None,
        EventKind::Alloc { ptr, .. } => return None,
        EventKind::AddrOfLocal(ptr, _) => return None,
        EventKind::Done => return None,
        _ => obj,
    })
}

pub fn event_to_node_kind(event: &Event) -> Option<NodeKind> {
    Some(match event.kind {
        EventKind::Alloc { .. } => NodeKind::Malloc(1),
        EventKind::Realloc { .. } => NodeKind::Malloc(1),
        EventKind::Free { .. } => NodeKind::Free,
        EventKind::CopyPtr(..) | EventKind::CopyRef => NodeKind::Copy,
        EventKind::Field(_, field) => NodeKind::Field(field.into()),
        EventKind::LoadAddr(..) => NodeKind::LoadAddr,
        EventKind::StoreAddr(..) => NodeKind::StoreAddr,
        EventKind::LoadValue(..) => NodeKind::LoadValue,
        EventKind::StoreValue(..) => NodeKind::StoreValue,
        EventKind::AddrOfLocal(_, l) => NodeKind::AddrOfLocal(Local::from(l)),
        EventKind::ToInt(_) => NodeKind::PtrToInt,
        EventKind::FromInt(_) => NodeKind::IntToPtr,
        EventKind::Ret(_) => return None,
        EventKind::Offset(_, offset, _) => NodeKind::Offset(offset),
        EventKind::Done => return None,
    })
}

fn update_provenance(
    provenances: &mut HashMap<usize, (GraphId, NodeId)>,
    event_kind: &EventKind,
    metadata: &EventMetadata,
    mapping: (GraphId, NodeId),
) {
    match event_kind {
        EventKind::Alloc { ptr, .. } => {
            provenances.insert(*ptr, mapping);
        }
        EventKind::CopyPtr(ptr) => {
            // only insert if not already there
            let res = provenances.try_insert(*ptr, mapping);
            if res.is_ok() {
                log::warn!("{:p} doesn't have a source", ptr);
            }
        }
        EventKind::Realloc { new_ptr, .. } => {
            provenances.insert(*new_ptr, mapping);
        }
        EventKind::Offset(_, _, new_ptr) => {
            provenances.insert(*new_ptr, mapping);
        }
        EventKind::CopyRef => {
            provenances.insert(metadata.destination.clone().unwrap().local.clone(), mapping);
        }
        EventKind::AddrOfLocal(ptr, _) => {
            provenances.insert(*ptr, mapping);
        }
        _ => (),
    }
}

pub fn add_node(
    graphs: &mut Graphs,
    provenances: &mut HashMap<usize, (GraphId, NodeId)>,
    event: &Event,
) -> Option<NodeId> {
    let node_kind = match event_to_node_kind(event) {
        Some(kind) => kind,
        None => return None,
    };

    let MirLoc {
        body_def,
        basic_block_idx,
        statement_idx,
        metadata,
    } = mir_loc::get(event.mir_loc).unwrap();

    if let EventKind::StoreAddr(_) = event.kind {
        println!("store: {:?}", metadata);
    }

    let function = DefPathHash(Fingerprint::new(body_def.0, body_def.1).into());
    let (hl, hr) = metadata.dest_func.into();
    let dest_fn = DefPathHash(Fingerprint::new(hl, hr).into());
    let source = metadata.source.as_ref().and_then(|dest| {
        graphs
            .latest_assignment
            .get(&(function, dest.local.clone()))
    });

    let ptr = get_ptr(&event.kind, &metadata)
        .and_then(|p| provenances.get(&p).cloned())
        .and_then(|(gid, last_nid_ref)| {
            graphs.graphs[gid]
                .nodes
                .iter()
                .rposition(|n| {
                    n.dest.is_some()
                        && n.dest.as_ref().map(|p| p.local)
                            == metadata.source.as_ref().map(|p| p.local)
                })
                .map(|nid| (gid, NodeId::from(nid)))
                .or(Some((gid, last_nid_ref)))
        });

    let node = Node {
        function: dest_fn,
        block: basic_block_idx.clone().into(),
        index: statement_idx.clone().into(),
        kind: node_kind,
        source: source
            .cloned()
            .and_then(|p| get_parent_object(&event.kind, p)),
        dest: metadata.destination.clone(),
    };

    let graph_id = ptr
        .and_then(|p| get_parent_object(&event.kind, p))
        .map(|(gid, _)| gid)
        .unwrap_or_else(|| graphs.graphs.push(Graph::new()));
    let node_id = graphs.graphs[graph_id].nodes.push(node);

    update_provenance(provenances, &event.kind, metadata, (graph_id, node_id));

    if let Some(dest) = &metadata.destination {
        let unique_place = (dest_fn, dest.local.clone());
        let last_setting = (graph_id, node_id);

        if let Some(last @ (last_gid, last_nid)) =
            graphs.latest_assignment.insert(unique_place, last_setting)
        {
            if !dest.projection.is_empty()
                && graphs.graphs[last_gid].nodes[last_nid]
                    .dest
                    .as_ref()
                    .unwrap()
                    .projection
                    .is_empty()
            {
                graphs.latest_assignment.insert(unique_place, last);
            }
        }
    }

    Some(node_id)
}

pub fn construct_pdg(events: &Vec<Event>) -> Graphs {
    let mut graphs = Graphs::new();
    let mut provenances = HashMap::<usize, (GraphId, NodeId)>::new();
    for event in events {
        add_node(&mut graphs, &mut provenances, event);
    }

    graphs
}
