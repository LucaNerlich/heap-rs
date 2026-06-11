mod common;

use common::OwnedFixture;
use heap_rs::{graph, index, retained, report};
use std::collections::HashMap;

#[test]
fn linked_list_indexing() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).expect("index build");

    assert_eq!(index.objects.len(), 3);
    assert_eq!(index.roots, vec![0x3000]);
    assert!(index.classes.contains_key(&0x2001));

    let names: HashMap<_, _> = index
        .objects
        .iter()
        .map(|o| (o.addr, o.class_name.clone()))
        .collect();
    assert_eq!(names[&0x3000], "com/example/Node");
}

#[test]
fn linked_list_graph_edges() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();
    let graph = graph::ObjectGraph::build(&hprof, &index, true).unwrap();

    assert_eq!(graph.num_nodes, 3);
    assert_eq!(graph.roots, vec![0]);
    assert_eq!(graph.targets.len(), 2);

    let root_edges: Vec<u32> = graph.targets[graph.offsets[0] as usize..graph.offsets[1] as usize]
        .to_vec();
    assert_eq!(root_edges, vec![1]);
}

#[test]
fn linked_list_retained_sizes() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();
    let graph = graph::ObjectGraph::build(&hprof, &index, true).unwrap();
    let analysis = retained::compute_retained(&graph, true);

    let root_row = analysis
        .top_objects
        .iter()
        .find(|o| o.addr == 0x3000)
        .expect("root object");
    let n1_row = analysis
        .top_objects
        .iter()
        .find(|o| o.addr == 0x3001)
        .expect("n1 object");
    let n2_row = analysis
        .top_objects
        .iter()
        .find(|o| o.addr == 0x3002)
        .expect("n2 object");

    assert_eq!(n2_row.retained_bytes, 24);
    assert_eq!(n1_row.retained_bytes, 24);
    assert_eq!(root_row.retained_bytes, 24);
    assert_eq!(analysis.reachable_objects + analysis.unreachable_objects, 3);
    assert_eq!(analysis.total_shallow, 72);
}

#[test]
fn holder_fixture_counts_objects_and_arrays() {
    let fixture = OwnedFixture::holder_and_array();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();

    assert_eq!(index.objects.len(), 6);
    assert_eq!(index.roots.len(), 1);

    let class_names: Vec<_> = index.objects.iter().map(|o| o.class_name.as_str()).collect();
    assert!(class_names.iter().any(|n| n.ends_with("[]")));
    assert!(class_names.iter().any(|n| *n == "int[]"));
}

#[test]
fn shallow_histogram_sums_match() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();
    let graph = graph::ObjectGraph::build(&hprof, &index, true).unwrap();

    let hist = graph.shallow_histogram();
    let total_count: u64 = hist.iter().map(|(_, c, _)| c).sum();
    let total_bytes: u64 = hist.iter().map(|(_, _, b)| b).sum();

    assert_eq!(total_count, 3);
    assert_eq!(total_bytes, 24 * 3);
}

#[test]
fn class_csv_roundtrip() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();
    let graph = graph::ObjectGraph::build(&hprof, &index, true).unwrap();
    let analysis = retained::compute_retained(&graph, true);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("classes.csv");
    report::write_class_csv(&path, &analysis.class_rows).unwrap();

    let parsed: Vec<(String, u64, u64, u64)> = csv::Reader::from_path(&path)
        .unwrap()
        .records()
        .map(|r| {
            let r = r.unwrap();
            (
                r[0].to_string(),
                r[1].parse().unwrap(),
                r[2].parse().unwrap(),
                r[3].parse().unwrap(),
            )
        })
        .collect();

    assert_eq!(parsed.len(), analysis.class_rows.len());
    assert!(parsed.iter().any(|(name, _, _, _)| name == "com/example/Node"));
}

#[test]
fn object_csv_roundtrip_respects_limit() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();
    let graph = graph::ObjectGraph::build(&hprof, &index, true).unwrap();
    let analysis = retained::compute_retained(&graph, true);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("objects.csv");
    report::write_object_csv(&path, &analysis.top_objects, Some(2)).unwrap();

    let content = std::fs::read_to_string(path).unwrap();
    let lines: Vec<_> = content.lines().collect();
    assert_eq!(lines.len(), 3);
}

#[test]
fn total_shallow_equals_sum_of_instance_sizes() {
    let fixture = OwnedFixture::linked_list();
    let hprof = fixture.parse();
    let index = index::HeapIndex::build(&hprof, true).unwrap();
    let graph = graph::ObjectGraph::build(&hprof, &index, true).unwrap();
    let analysis = retained::compute_retained(&graph, true);

    assert_eq!(analysis.total_shallow, 72);
}