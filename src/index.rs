use crate::progress::ProgressGroup;
use jvm_hprof::heap_dump::{FieldType, FieldValue, Instance, PrimitiveArrayType, SubRecord};
use jvm_hprof::{Hprof, IdSize, RecordTag};
use rustc_hash::FxHashMap;
use std::time::Instant;

pub struct ClassLayout {
    pub fields: Vec<FieldType>,
}

pub struct ObjectMeta {
    pub addr: u64,
    pub shallow: u32,
    pub class_name: String,
}

pub struct HeapIndex {
    pub id_size: IdSize,
    pub classes: FxHashMap<u64, ClassLayout>,
    pub objects: Vec<ObjectMeta>,
    pub roots: Vec<u64>,
}

impl HeapIndex {
    pub fn build(hprof: &Hprof<'_>, quiet: bool) -> Result<Self, String> {
        let id_size = hprof.header().id_size();
        let group = ProgressGroup::new("Pass 1: indexing heap", 3, quiet);
        let started = Instant::now();
        let mut utf8 = FxHashMap::default();
        let mut load_class_names = FxHashMap::default();
        let mut raw_classes: FxHashMap<u64, (Option<u64>, u32, Vec<FieldType>)> =
            FxHashMap::default();
        let mut objects = Vec::new();
        let mut roots = Vec::new();

        let mut progress = group.begin(1, "loading symbols");
        let mut in_heap = false;

        for record in hprof.records_iter() {
            let record = record.map_err(|e| format!("{e:?}"))?;
            match record.tag() {
                RecordTag::Utf8 => {
                    let parsed = record
                        .as_utf_8()
                        .ok_or_else(|| "expected utf8 record".to_string())?
                        .map_err(|e| format!("{e:?}"))?;
                    utf8.insert(
                        parsed.name_id().id(),
                        parsed
                            .text_as_str()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|_| String::from_utf8_lossy(parsed.text()).into_owned()),
                    );
                    progress.add_nodes(1);
                }
                RecordTag::LoadClass => {
                    let lc = record
                        .as_load_class()
                        .ok_or_else(|| "expected load class".to_string())?
                        .map_err(|e| format!("{e:?}"))?;
                    if let Some(name) = utf8.get(&lc.class_name_id().id()) {
                        load_class_names.insert(lc.class_obj_id().id(), name.clone());
                    }
                    progress.add_nodes(1);
                }
                RecordTag::HeapDump | RecordTag::HeapDumpSegment => {
                    if !in_heap {
                        progress.finish(format!(
                            "{} strings, {} classes loaded",
                            utf8.len(),
                            load_class_names.len()
                        ));
                        progress = group.begin(2, "scanning heap");
                        in_heap = true;
                    }
                    progress.tick_segment();
                    let seg = record
                        .as_heap_dump_segment()
                        .ok_or_else(|| "expected heap dump".to_string())?
                        .map_err(|e| format!("{e:?}"))?;
                    for sub in seg.sub_records() {
                        let sub = sub.map_err(|e| format!("{e:?}"))?;
                        progress.tick_sub_record();
                        match sub {
                            SubRecord::Class(c) => {
                                progress.add_class();
                                let mut fields = Vec::new();
                                for fd in c.instance_field_descriptors() {
                                    fields.push(
                                        fd.map_err(|e| format!("{e:?}"))?
                                            .field_type(),
                                    );
                                }
                                raw_classes.insert(
                                    c.obj_id().id(),
                                    (
                                        c.super_class_obj_id().map(|id| id.id()),
                                        c.instance_size_bytes(),
                                        fields,
                                    ),
                                );
                            }
                            SubRecord::Instance(inst) => {
                                progress.add_object();
                                let class_id = inst.class_obj_id().id();
                                let name = class_name(class_id, &load_class_names);
                                let shallow = raw_classes
                                    .get(&class_id)
                                    .map(|(_, sz, _)| *sz)
                                    .unwrap_or(inst.fields().len() as u32);
                                objects.push(ObjectMeta {
                                    addr: inst.obj_id().id(),
                                    shallow,
                                    class_name: name,
                                });
                            }
                            SubRecord::ObjectArray(arr) => {
                                progress.add_object();
                                let name = array_class_name(
                                    arr.array_class_obj_id().id(),
                                    &load_class_names,
                                );
                                let ne = arr.elements(id_size).count() as u32;
                                let shallow = array_shallow(ne, id_bytes(id_size));
                                objects.push(ObjectMeta {
                                    addr: arr.obj_id().id(),
                                    shallow,
                                    class_name: name,
                                });
                            }
                            SubRecord::PrimitiveArray(arr) => {
                                progress.add_object();
                                let ne = primitive_array_len(&arr);
                                let shallow =
                                    array_shallow(ne, primitive_elem_size(arr.primitive_type()));
                                objects.push(ObjectMeta {
                                    addr: arr.obj_id().id(),
                                    shallow,
                                    class_name: format!(
                                        "{}[]",
                                        arr.primitive_type().java_type_name()
                                    ),
                                });
                            }
                            SubRecord::GcRootUnknown(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootJniGlobal(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootJniLocalRef(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootJavaStackFrame(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootNativeStack(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootSystemClass(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootThreadBlock(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootBusyMonitor(r) => {
                                progress.add_root();
                                roots.push(r.obj_id().id());
                            }
                            SubRecord::GcRootThreadObj(r) => {
                                if let Some(id) = r.thread_obj_id() {
                                    progress.add_root();
                                    roots.push(id.id());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if in_heap {
            progress.finish(format!(
                "{} objects, {} class dumps, {} roots",
                objects.len(),
                raw_classes.len(),
                roots.len()
            ));
        } else {
            progress.finish(format!(
                "{} strings, {} classes loaded",
                utf8.len(),
                load_class_names.len()
            ));
        }

        let mut progress = group.begin(3, "finalizing class layouts");
        let mut classes = FxHashMap::default();
        for &class_id in raw_classes.keys() {
            let layout = build_class_layout(class_id, &raw_classes, &load_class_names);
            classes.insert(class_id, layout);
            progress.add_nodes(1);
        }

        roots.sort_unstable();
        roots.dedup();

        progress.finish(format!(
            "Pass 1 done: {} objects, {} classes, {} roots in {:.1?}",
            objects.len(),
            classes.len(),
            roots.len(),
            started.elapsed()
        ));

        Ok(HeapIndex {
            id_size,
            classes,
            objects,
            roots,
        })
    }

    pub fn extract_refs(&self, sub: &SubRecord<'_>) -> Result<Vec<u64>, String> {
        let id_size = self.id_size;
        match sub {
            SubRecord::Instance(inst) => instance_refs(inst, id_size, &self.classes),
            SubRecord::ObjectArray(arr) => {
                let mut refs = Vec::new();
                for elem in arr.elements(id_size) {
                    if let Some(id) = elem.map_err(|e| format!("{e:?}"))? {
                        refs.push(id.id());
                    }
                }
                Ok(refs)
            }
            _ => Ok(Vec::new()),
        }
    }
}

fn id_bytes(id_size: IdSize) -> u8 {
    match id_size {
        IdSize::U32 => 4,
        IdSize::U64 => 8,
    }
}

fn build_class_layout(
    class_id: u64,
    raw: &FxHashMap<u64, (Option<u64>, u32, Vec<FieldType>)>,
    _names: &FxHashMap<u64, String>,
) -> ClassLayout {
    let mut chain = Vec::new();
    let mut cur = Some(class_id);
    let mut seen = FxHashMap::default();
    while let Some(cid) = cur {
        if seen.insert(cid, ()).is_some() {
            break;
        }
        chain.push(cid);
        cur = raw.get(&cid).and_then(|(sup, _, _)| *sup);
    }

    let mut fields = Vec::new();

    for &cid in chain.iter().rev() {
        if let Some((_, _, local)) = raw.get(&cid) {
            fields.extend(local.iter().copied());
        }
    }

    ClassLayout { fields }
}

fn class_name(class_id: u64, names: &FxHashMap<u64, String>) -> String {
    names
        .get(&class_id)
        .cloned()
        .unwrap_or_else(|| format!("0x{class_id:x}"))
}

fn array_class_name(class_id: u64, names: &FxHashMap<u64, String>) -> String {
    names
        .get(&class_id)
        .map(|n| format!("{n}[]"))
        .unwrap_or_else(|| format!("0x{class_id:x}[]"))
}

fn primitive_elem_size(t: PrimitiveArrayType) -> u8 {
    match t {
        PrimitiveArrayType::Boolean | PrimitiveArrayType::Byte => 1,
        PrimitiveArrayType::Char | PrimitiveArrayType::Short => 2,
        PrimitiveArrayType::Float | PrimitiveArrayType::Int => 4,
        PrimitiveArrayType::Double | PrimitiveArrayType::Long => 8,
    }
}

fn primitive_array_len(arr: &jvm_hprof::heap_dump::PrimitiveArray<'_>) -> u32 {
    if let Some(it) = arr.booleans() {
        return it.count() as u32;
    }
    if let Some(it) = arr.chars() {
        return it.count() as u32;
    }
    if let Some(it) = arr.floats() {
        return it.count() as u32;
    }
    if let Some(it) = arr.doubles() {
        return it.count() as u32;
    }
    if let Some(it) = arr.bytes() {
        return it.count() as u32;
    }
    if let Some(it) = arr.shorts() {
        return it.count() as u32;
    }
    if let Some(it) = arr.ints() {
        return it.count() as u32;
    }
    if let Some(it) = arr.longs() {
        return it.count() as u32;
    }
    0
}

fn array_shallow(num_elements: u32, elem_size: u8) -> u32 {
    let payload = num_elements as u64 * elem_size as u64;
    let total = 16u64 + payload;
    ((total + 7) & !7) as u32
}

fn instance_refs(
    inst: &Instance<'_>,
    id_size: IdSize,
    classes: &FxHashMap<u64, ClassLayout>,
) -> Result<Vec<u64>, String> {
    let Some(layout) = classes.get(&inst.class_obj_id().id()) else {
        return Ok(Vec::new());
    };
    let mut refs = Vec::new();
    let mut input: &[u8] = inst.fields();
    for ft in &layout.fields {
        let (rest, val) = ft
            .parse_value(input, id_size)
            .map_err(|e| format!("{e:?}"))?;
        input = rest;
        if let FieldValue::ObjectId(Some(id)) = val {
            refs.push(id.id());
        }
    }
    Ok(refs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::hprof::OwnedFixture;

    #[test]
    fn builds_class_layout_with_superclass_fields() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = HeapIndex::build(&hprof, true).unwrap();
        let node_layout = index.classes.get(&0x2001).expect("Node class");
        assert_eq!(node_layout.fields.len(), 1);
    }

    #[test]
    fn deduplicates_roots() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = HeapIndex::build(&hprof, true).unwrap();
        assert_eq!(index.roots, vec![0x3000]);
    }

    #[test]
    fn extract_refs_from_linked_instance() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = HeapIndex::build(&hprof, true).unwrap();
        for record in hprof.records_iter().flatten() {
            if let Some(seg) = record.as_heap_dump_segment() {
                let seg = seg.unwrap();
                for sub in seg.sub_records().flatten() {
                    if let jvm_hprof::heap_dump::SubRecord::Instance(ref inst) = sub {
                        if inst.obj_id().id() == 0x3000 {
                            let refs = index.extract_refs(&sub).unwrap();
                            assert_eq!(refs, vec![0x3001]);
                            return;
                        }
                    }
                }
            }
        }
        panic!("root instance not found");
    }
}
