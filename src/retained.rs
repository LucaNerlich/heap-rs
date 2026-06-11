use crate::dominators::compute_dominators;
use crate::graph::ObjectGraph;
use crate::progress::ProgressGroup;
use rayon::prelude::*;

pub struct ClassRetainedRow {
    pub class_name: String,
    pub instance_count: u64,
    pub shallow_bytes: u64,
    pub retained_bytes: u64,
}

pub struct ObjectRetainedRow {
    pub addr: u64,
    pub class_name: String,
    pub shallow_bytes: u64,
    pub retained_bytes: u64,
}

pub struct RetainedAnalysis {
    pub class_rows: Vec<ClassRetainedRow>,
    pub top_objects: Vec<ObjectRetainedRow>,
    pub total_shallow: u64,
    pub total_retained: u64,
    pub reachable_objects: u64,
    pub unreachable_objects: u64,
}

pub fn compute_retained(graph: &ObjectGraph, quiet: bool) -> RetainedAnalysis {
    let group = ProgressGroup::new("Computing retained sizes", 3, quiet);

    let progress = group.begin(1, "dominator tree");
    let n = graph.num_nodes;
    let super_root = graph.super_root as usize;
    let idom = compute_dominators(graph);
    progress.finish(format!("Dominator tree for {} nodes", n));

    let mut progress = group.begin(2, "accumulating retained sizes");
    let mut dom_children: Vec<Vec<u32>> = vec![Vec::new(); n + 1];
    for v in 0..n {
        let d = idom[v] as usize;
        if d != v && d != super_root {
            dom_children[d].push(v as u32);
        } else if d == super_root && v != super_root {
            dom_children[super_root].push(v as u32);
        }
    }

    let mut depth = vec![0u32; n + 1];
    let mut stack = vec![graph.super_root];
    while let Some(v) = stack.pop() {
        for &c in &dom_children[v as usize] {
            depth[c as usize] = depth[v as usize] + 1;
            stack.push(c);
        }
    }

    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&v| std::cmp::Reverse(depth[v]));

    let mut retained = vec![0u64; n];
    for &v in &order {
        retained[v] = graph.shallow[v] as u64;
        for &c in &dom_children[v] {
            retained[v] += retained[c as usize];
        }
        progress.add_nodes(1);
    }
    progress.finish(format!(
        "Retained sizes for {} objects",
        format_count(n as u64)
    ));

    let progress = group.begin(3, "aggregating by class");
    let k = graph.class_names.len();
    let mut class_count = vec![0u64; k];
    let mut class_shallow = vec![0u64; k];
    let mut class_retained = vec![0u64; k];

    let mut reachable = 0u64;
    let mut unreachable = 0u64;
    let mut total_shallow = 0u64;

    for v in 0..n {
        total_shallow += graph.shallow[v] as u64;
        let c = graph.object_class[v] as usize;
        class_count[c] += 1;
        class_shallow[c] += graph.shallow[v] as u64;
        class_retained[c] += retained[v];

        if depth[v] > 0 {
            reachable += 1;
        } else {
            unreachable += 1;
        }
    }

    let mut class_rows: Vec<ClassRetainedRow> = graph
        .class_names
        .iter()
        .enumerate()
        .map(|(i, name)| ClassRetainedRow {
            class_name: name.clone(),
            instance_count: class_count[i],
            shallow_bytes: class_shallow[i],
            retained_bytes: class_retained[i],
        })
        .collect();
    class_rows.par_sort_unstable_by(|a, b| b.retained_bytes.cmp(&a.retained_bytes));

    let mut object_rows: Vec<ObjectRetainedRow> = (0..n)
        .map(|v| ObjectRetainedRow {
            addr: graph.addrs[v],
            class_name: graph.class_names[graph.object_class[v] as usize].clone(),
            shallow_bytes: graph.shallow[v] as u64,
            retained_bytes: retained[v],
        })
        .collect();
    object_rows.par_sort_unstable_by(|a, b| b.retained_bytes.cmp(&a.retained_bytes));

    let total_retained = if !graph.roots.is_empty() {
        retained
            .iter()
            .enumerate()
            .filter(|(v, _)| depth[*v] > 0)
            .map(|(_, &r)| r)
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    progress.finish(format!(
        "{} classes, {} reachable objects",
        format_count(k as u64),
        format_count(reachable)
    ));

    RetainedAnalysis {
        class_rows,
        top_objects: object_rows,
        total_shallow,
        total_retained,
        reachable_objects: reachable,
        unreachable_objects: unreachable,
    }
}

fn format_count(n: u64) -> String {
    crate::progress::format_count(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::hprof::OwnedFixture;

    #[test]
    fn retained_per_object_reports_shallow_sizes() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = crate::graph::ObjectGraph::build(&hprof, &index, true).unwrap();
        let analysis = compute_retained(&graph, true);

        let by_addr: std::collections::HashMap<_, _> = analysis
            .top_objects
            .iter()
            .map(|o| (o.addr, o.retained_bytes))
            .collect();
        assert_eq!(by_addr[&0x3002], 24);
        assert_eq!(by_addr[&0x3001], 24);
        assert_eq!(by_addr[&0x3000], 24);
        assert_eq!(analysis.total_shallow, 72);
    }

    #[test]
    fn class_rows_aggregate_instance_counts() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = crate::graph::ObjectGraph::build(&hprof, &index, true).unwrap();
        let analysis = compute_retained(&graph, true);

        assert_eq!(analysis.class_rows.len(), 1);
        assert_eq!(analysis.class_rows[0].instance_count, 3);
        assert_eq!(analysis.class_rows[0].shallow_bytes, 72);
    }

    #[test]
    fn holder_fixture_object_count() {
        let fixture = OwnedFixture::holder_and_array();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = crate::graph::ObjectGraph::build(&hprof, &index, true).unwrap();
        let analysis = compute_retained(&graph, true);

        assert_eq!(analysis.reachable_objects + analysis.unreachable_objects, 6);
        assert_eq!(analysis.total_shallow, index.objects.iter().map(|o| o.shallow as u64).sum());
    }
}
