//! The object reference graph.
//!
//! [`ObjectGraph`](crate::graph::ObjectGraph) stores nodes (objects) and directed edges (references) in
//! [compressed-sparse-row](https://en.wikipedia.org/wiki/Sparse_matrix#Compressed_sparse_row_(CSR,_CRS_or_Yale_format))
//! form. Objects are identified by a dense node id `0..num_nodes`, assigned by
//! sorting addresses, so the heavy graph algorithms operate on small contiguous
//! integer arrays rather than 64-bit addresses.

use crate::index::HeapIndex;
use crate::progress::{format_count, ProgressGroup};
use jvm_hprof::heap_dump::SubRecord;
use jvm_hprof::{Hprof, RecordTag};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::time::Instant;

/// The object reference graph in compressed-sparse-row (CSR) form.
///
/// Each object is a node identified by a dense id in `0..num_nodes`, assigned by
/// sorting addresses ascending. The per-node arrays ([`addrs`](Self::addrs),
/// [`shallow`](Self::shallow), [`object_class`](Self::object_class)) are indexed
/// by that id. Edges are stored as CSR: the outgoing edges of node `v` are
/// `targets[offsets[v]..offsets[v + 1]]`.
///
/// [`super_root`](Self::super_root) (`= num_nodes`) is a synthetic node that
/// points at every GC root, giving dominator analysis a single entry point.
pub struct ObjectGraph {
    /// Object addresses, sorted ascending; `addrs[id]` is node `id`'s address.
    pub addrs: Vec<u64>,
    /// Shallow size in bytes per node.
    pub shallow: Vec<u64>,
    /// Interned class names; indexed by the values in [`object_class`](Self::object_class).
    pub class_names: Vec<String>,
    /// Class index per node, pointing into [`class_names`](Self::class_names).
    pub object_class: Vec<u32>,
    /// CSR row offsets; node `v`'s edges span `offsets[v]..offsets[v + 1]`. Length is `num_nodes + 1`.
    pub offsets: Vec<u32>,
    /// CSR edge targets (destination node ids), concatenated per source node.
    pub targets: Vec<u32>,
    /// GC root node ids, sorted and deduplicated.
    pub roots: Vec<u32>,
    /// Number of real object nodes (excludes the synthetic super-root).
    pub num_nodes: usize,
    /// Id of the synthetic super-root node (`= num_nodes`) that links to all GC roots.
    pub super_root: u32,
}

impl ObjectGraph {
    /// Build the CSR object graph from a parsed dump and its [`HeapIndex`].
    ///
    /// Sorts object addresses to assign dense node ids, maps per-object
    /// metadata, then scans the heap once to collect edges before sorting them
    /// into CSR layout. Address mapping, sorting, and CSR fill run in parallel.
    ///
    /// Set `quiet` to `true` to suppress progress spinners.
    ///
    /// # Errors
    ///
    /// Returns an `Err(String)` if a heap record fails to parse.
    pub fn build(hprof: &Hprof<'_>, index: &HeapIndex, quiet: bool) -> Result<Self, String> {
        let n = index.objects.len();
        let started = Instant::now();
        let group = ProgressGroup::new("Building object graph", 4, quiet);

        let progress = group.begin(1, "sorting addresses");
        let mut addrs: Vec<u64> = index.objects.iter().map(|o| o.addr).collect();
        addrs.par_sort_unstable();
        progress.finish(format!("Sorted {} object addresses", format_count(n as u64)));

        let progress = group.begin(2, "mapping object metadata");
        let addr_to_id: FxHashMap<u64, u32> = addrs
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, i as u32))
            .collect();

        let mut class_names: Vec<String> = Vec::new();
        let mut class_name_to_idx: FxHashMap<String, u32> = FxHashMap::default();
        for obj in &index.objects {
            class_name_to_idx
                .entry(obj.class_name.clone())
                .or_insert_with(|| {
                    let idx = class_names.len() as u32;
                    class_names.push(obj.class_name.clone());
                    idx
                });
        }

        let mut shallow = vec![0u64; n];
        let mut object_class = vec![0u32; n];
        let metadata: Vec<(usize, u64, u32)> = index
            .objects
            .par_iter()
            .map(|obj| {
                let id = addr_to_id[&obj.addr] as usize;
                (id, obj.shallow, class_name_to_idx[&obj.class_name])
            })
            .collect();
        for (id, s, c) in metadata {
            shallow[id] = s;
            object_class[id] = c;
        }

        let mut roots: Vec<u32> = index
            .roots
            .iter()
            .filter_map(|&a| addr_to_id.get(&a).copied())
            .collect();
        roots.par_sort_unstable();
        roots.dedup();
        progress.finish(format!(
            "{} objects, {} classes, {} roots mapped",
            format_count(n as u64),
            format_count(class_names.len() as u64),
            format_count(roots.len() as u64)
        ));

        let mut progress = group.begin(3, "collecting edges");
        let collect_started = Instant::now();
        let mut edge_list: Vec<(u32, u32)> = Vec::new();

        for record in hprof.records_iter() {
            let record = record.map_err(|e| format!("{e:?}"))?;
            if !matches!(record.tag(), RecordTag::HeapDump | RecordTag::HeapDumpSegment) {
                continue;
            }
            progress.tick_segment();
            let seg = record
                .as_heap_dump_segment()
                .ok_or_else(|| "expected heap dump".to_string())?
                .map_err(|e| format!("{e:?}"))?;
            for sub in seg.sub_records() {
                let sub = sub.map_err(|e| format!("{e:?}"))?;
                progress.tick_sub_record();
                let (from_id, refs) = match &sub {
                    SubRecord::Instance(inst) => {
                        (inst.obj_id().id(), index.extract_refs(&sub)?)
                    }
                    SubRecord::ObjectArray(arr) => {
                        (arr.obj_id().id(), index.extract_refs(&sub)?)
                    }
                    _ => continue,
                };
                let Some(&from) = addr_to_id.get(&from_id) else {
                    continue;
                };
                let mut edge_batch = 0u64;
                for addr in refs {
                    if let Some(&target) = addr_to_id.get(&addr) {
                        edge_list.push((from, target));
                        edge_batch += 1;
                    }
                }
                progress.add_edges(edge_batch);
            }
        }

        progress.finish(format!(
            "Collected {} edges in {:.1?}",
            format_count(edge_list.len() as u64),
            collect_started.elapsed()
        ));

        let progress = group.begin(4, "building CSR adjacency");
        edge_list.par_sort_unstable_by_key(|&(from, _)| from);

        let m = edge_list.len();
        let mut offsets = vec![0u32; n + 1];
        let mut idx = 0usize;
        for from in 0..n {
            offsets[from] = idx as u32;
            while idx < m && edge_list[idx].0 == from as u32 {
                idx += 1;
            }
        }
        offsets[n] = idx as u32;

        let targets: Vec<u32> = edge_list.par_iter().map(|&(_, to)| to).collect();

        progress.finish(format!(
            "Object graph done: {} edges, {} objects in {:.1?}",
            format_count(offsets[n] as u64),
            format_count(n as u64),
            started.elapsed()
        ));

        Ok(ObjectGraph {
            addrs,
            shallow,
            class_names,
            object_class,
            offsets,
            targets,
            roots,
            num_nodes: n,
            super_root: n as u32,
        })
    }

    /// Aggregate shallow size by class, sorted by total bytes descending.
    ///
    /// Returns one `(class_name, instance_count, shallow_bytes)` tuple per
    /// class. This is a cheap summary that does not require the dominator tree,
    /// so it can be produced in `--shallow-only` mode. The counting is
    /// parallelized with a per-thread fold/reduce.
    pub fn shallow_histogram(&self) -> Vec<(String, u64, u64)> {
        let k = self.class_names.len();
        if k == 0 {
            return Vec::new();
        }

        let (counts, bytes) = (0..self.num_nodes)
            .into_par_iter()
            .fold(
                || (vec![0u64; k], vec![0u64; k]),
                |mut acc, i| {
                    let c = self.object_class[i] as usize;
                    acc.0[c] += 1;
                    acc.1[c] += self.shallow[i] as u64;
                    acc
                },
            )
            .reduce(
                || (vec![0u64; k], vec![0u64; k]),
                |mut a, b| {
                    for i in 0..k {
                        a.0[i] += b.0[i];
                        a.1[i] += b.1[i];
                    }
                    a
                },
            );

        let mut rows: Vec<(String, u64, u64)> = self
            .class_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), counts[i], bytes[i]))
            .collect();
        rows.par_sort_unstable_by(|a, b| b.2.cmp(&a.2));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::hprof::OwnedFixture;

    #[test]
    fn csr_offsets_match_edge_count() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = ObjectGraph::build(&hprof, &index, true).unwrap();
        assert_eq!(graph.offsets[graph.num_nodes] as usize, graph.targets.len());
    }

    #[test]
    fn sorted_addresses_are_monotonic() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = ObjectGraph::build(&hprof, &index, true).unwrap();
        for window in graph.addrs.windows(2) {
            assert!(window[0] <= window[1]);
        }
    }

    #[test]
    fn shallow_histogram_groups_by_class() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = ObjectGraph::build(&hprof, &index, true).unwrap();
        let hist = graph.shallow_histogram();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].0, "com/example/Node");
        assert_eq!(hist[0].1, 3);
    }
}
