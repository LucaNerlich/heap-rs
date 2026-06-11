use crate::index::HeapIndex;
use crate::progress::{format_count, ProgressGroup};
use jvm_hprof::heap_dump::SubRecord;
use jvm_hprof::{Hprof, RecordTag};
use rayon::prelude::*;
use rustc_hash::FxHashMap;
use std::time::Instant;

pub struct ObjectGraph {
    pub addrs: Vec<u64>,
    pub shallow: Vec<u32>,
    pub class_names: Vec<String>,
    pub object_class: Vec<u32>,
    pub offsets: Vec<u32>,
    pub targets: Vec<u32>,
    pub roots: Vec<u32>,
    pub num_nodes: usize,
    pub super_root: u32,
}

impl ObjectGraph {
    pub fn build(hprof: &Hprof<'_>, index: &HeapIndex, quiet: bool) -> Result<Self, String> {
        let n = index.objects.len();
        let started = Instant::now();
        let group = ProgressGroup::new("Building object graph", 5, quiet);

        let progress = group.begin(1, "sorting addresses");
        let mut addrs: Vec<u64> = index.objects.iter().map(|o| o.addr).collect();
        addrs.par_sort_unstable();
        progress.finish(format!("Sorted {} object addresses", format_count(n as u64)));

        let mut progress = group.begin(2, "building address index");
        let addr_to_id: FxHashMap<u64, u32> = addrs
            .iter()
            .enumerate()
            .map(|(i, &a)| (a, i as u32))
            .collect();

        let mut shallow = vec![0u32; n];
        let mut object_class = vec![0u32; n];
        let mut class_names: Vec<String> = Vec::new();
        let mut class_name_to_idx: FxHashMap<String, u32> = FxHashMap::default();

        for obj in &index.objects {
            let id = addr_to_id[&obj.addr] as usize;
            shallow[id] = obj.shallow;
            let class_idx = *class_name_to_idx
                .entry(obj.class_name.clone())
                .or_insert_with(|| {
                    let idx = class_names.len() as u32;
                    class_names.push(obj.class_name.clone());
                    idx
                });
            object_class[id] = class_idx;
            progress.add_nodes(1);
        }

        let mut roots: Vec<u32> = index
            .roots
            .iter()
            .filter_map(|&a| addr_to_id.get(&a).copied())
            .collect();
        roots.sort_unstable();
        roots.dedup();
        progress.finish(format!(
            "{} objects, {} classes, {} roots mapped",
            format_count(n as u64),
            format_count(class_names.len() as u64),
            format_count(roots.len() as u64)
        ));

        let mut progress = group.begin(3, "counting edges");
        let count_started = Instant::now();
        let mut offsets = vec![0u32; n + 1];

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
                    if addr_to_id.contains_key(&addr) {
                        offsets[from as usize + 1] += 1;
                        edge_batch += 1;
                    }
                }
                progress.add_edges(edge_batch);
            }
        }

        progress.finish(format!(
            "Counted {} edges in {:.1?}",
            format_count(offsets[n] as u64),
            count_started.elapsed()
        ));

        let progress = group.begin(4, "allocating edge buffer");
        for i in 0..n {
            offsets[i + 1] += offsets[i];
        }
        let mut targets = vec![0u32; offsets[n] as usize];
        let mut write_pos = offsets.clone();
        progress.finish(format!(
            "Allocated buffer for {} edges",
            format_count(offsets[n] as u64)
        ));

        let mut progress = group.begin(5, "writing edges");
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
                        let pos = write_pos[from as usize] as usize;
                        targets[pos] = target;
                        write_pos[from as usize] += 1;
                        edge_batch += 1;
                    }
                }
                progress.add_edges(edge_batch);
            }
        }

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

    pub fn shallow_histogram(&self) -> Vec<(String, u64, u64)> {
        let k = self.class_names.len();
        let mut counts = vec![0u64; k];
        let mut bytes = vec![0u64; k];
        for i in 0..self.num_nodes {
            let c = self.object_class[i] as usize;
            counts[c] += 1;
            bytes[c] += self.shallow[i] as u64;
        }
        let mut rows: Vec<(String, u64, u64)> = self
            .class_names
            .iter()
            .enumerate()
            .map(|(i, name)| (name.clone(), counts[i], bytes[i]))
            .collect();
        rows.sort_by(|a, b| b.2.cmp(&a.2));
        rows
    }
}
