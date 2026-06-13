//! Retained-size aggregation and class retainer analysis.
//!
//! Given an [`ObjectGraph`](crate::graph::ObjectGraph),
//! [`compute_retained`](crate::retained::compute_retained) builds the dominator
//! tree, sums each object's retained size (its shallow size plus everything it
//! dominates), and rolls the totals up per class.
//! [`explain_class`](crate::retained::explain_class) answers the complementary
//! question — *who references instances of a given class?* — which is what you
//! reach for when a leaf type such as `byte[]` dominates the heap.

use crate::dominators::compute_dominators;
use crate::graph::ObjectGraph;
use crate::progress::ProgressGroup;
use rayon::prelude::*;

/// One row of the per-class retained-size table.
pub struct ClassRetainedRow {
    /// Fully qualified class name.
    pub class_name: String,
    /// Number of live instances of this class.
    pub instance_count: u64,
    /// Summed shallow size of all instances.
    pub shallow_bytes: u64,
    /// Summed retained size of all instances.
    pub retained_bytes: u64,
}

/// One row of the per-object retained-size table.
pub struct ObjectRetainedRow {
    /// Object address (identity) in the dump.
    pub addr: u64,
    /// Class name of this object.
    pub class_name: String,
    /// Shallow size of this object.
    pub shallow_bytes: u64,
    /// Retained size of this object (shallow plus everything it dominates).
    pub retained_bytes: u64,
}

/// The full result of [`compute_retained`].
pub struct RetainedAnalysis {
    /// Per-class rows, sorted by retained size descending.
    pub class_rows: Vec<ClassRetainedRow>,
    /// Per-object rows, sorted by retained size descending.
    pub top_objects: Vec<ObjectRetainedRow>,
    /// Sum of all shallow sizes in the heap.
    pub total_shallow: u64,
    /// Largest single retained subtree (the heaviest dominator).
    pub total_retained: u64,
    /// Count of objects reachable from a GC root.
    pub reachable_objects: u64,
    /// Count of objects not reachable from any GC root.
    pub unreachable_objects: u64,
    /// Immediate-dominator array as returned by [`compute_dominators`].
    pub idom: Vec<u32>,
}

/// A class that holds incoming references to the target class, with totals.
pub struct RetainerRow {
    /// Class name of the referring (retaining) objects.
    pub retainer_class: String,
    /// Number of references from this class to instances of the target class.
    pub instance_count: u64,
    /// Summed shallow size of the referenced target instances.
    pub shallow_bytes: u64,
}

/// A single instance of the target class, with a representative referrer.
#[derive(Clone)]
pub struct ExplainedInstance {
    /// Address of this instance.
    pub addr: u64,
    /// Shallow size of this instance.
    pub shallow_bytes: u64,
    /// Class name of a representative object that references this instance
    /// (the shallowest predecessor), or `"GC root"` / `"(no incoming refs)"`.
    pub retainer_class: String,
    /// Address of the representative referrer, or `0` when there is none.
    pub retainer_addr: u64,
}

/// The result of [`explain_class`]: why a class occupies memory.
pub struct ClassExplanation {
    /// The resolved fully qualified class name.
    pub class_name: String,
    /// Number of matched instances.
    pub instance_count: u64,
    /// Summed shallow size of matched instances.
    pub total_shallow: u64,
    /// Largest matched instances, sorted by shallow size descending.
    pub top_instances: Vec<ExplainedInstance>,
    /// Classes that reference the matched instances, sorted by shallow size descending.
    pub top_retainers: Vec<RetainerRow>,
}

/// Test whether a class name matches a user-supplied filter.
///
/// A match occurs when the filter equals the full class name, equals the
/// segment after the last `/` (so `HashMap` matches `java/util/HashMap`), or is
/// an exact array-type match such as `byte[]`.
pub fn class_matches(class_name: &str, filter: &str) -> bool {
    class_name == filter || class_name.ends_with(&format!("/{filter}"))
}

/// Explain which objects reference instances of `class_filter`.
///
/// This walks **incoming** references (predecessors) rather than the dominator
/// tree, which is what you want for leaf types like `byte[]` where retained
/// size equals shallow size and the interesting question is *who keeps these
/// alive*. For each matched instance the shallowest predecessor is recorded as
/// a representative retainer, and referrers are aggregated by class.
///
/// `top_instances` and `top_retainers` cap the number of rows returned in
/// [`ClassExplanation::top_instances`] and [`ClassExplanation::top_retainers`].
///
/// Returns `None` if no object matches the filter.
pub fn explain_class(
    graph: &ObjectGraph,
    class_filter: &str,
    top_instances: usize,
    top_retainers: usize,
) -> Option<ClassExplanation> {
    let mut preds: Vec<Vec<u32>> = vec![Vec::new(); graph.num_nodes];
    for v in 0..graph.num_nodes {
        let start = graph.offsets[v] as usize;
        let end = graph.offsets[v + 1] as usize;
        for &w in &graph.targets[start..end] {
            preds[w as usize].push(v as u32);
        }
    }
    for &r in &graph.roots {
        preds[r as usize].push(graph.super_root);
    }

    let matched: Vec<(ExplainedInstance, Vec<(String, u64)>)> = (0..graph.num_nodes)
        .into_par_iter()
        .filter_map(|v| {
            let name = &graph.class_names[graph.object_class[v] as usize];
            if !class_matches(name, class_filter) {
                return None;
            }
            let shallow = graph.shallow[v] as u64;

            let (retainer_class, retainer_addr) = if preds[v].is_empty() {
                ("(no incoming refs)".to_string(), 0)
            } else {
                let &dom = preds[v]
                    .iter()
                    .min_by_key(|&&u| {
                        if u == graph.super_root {
                            u64::MAX
                        } else {
                            graph.shallow[u as usize] as u64
                        }
                    })
                    .unwrap_or(&preds[v][0]);
                if dom == graph.super_root {
                    ("GC root".to_string(), 0)
                } else {
                    (
                        graph.class_names[graph.object_class[dom as usize] as usize].clone(),
                        graph.addrs[dom as usize],
                    )
                }
            };

            let retainer_hits: Vec<(String, u64)> = preds[v]
                .iter()
                .map(|&pred| {
                    let rc = if pred == graph.super_root {
                        "GC root".to_string()
                    } else {
                        graph.class_names[graph.object_class[pred as usize] as usize].clone()
                    };
                    (rc, shallow)
                })
                .collect();

            Some((
                ExplainedInstance {
                    addr: graph.addrs[v],
                    shallow_bytes: shallow,
                    retainer_class,
                    retainer_addr,
                },
                retainer_hits,
            ))
        })
        .collect();

    if matched.is_empty() {
        return None;
    }

    let class_name = (0..graph.num_nodes)
        .find(|&v| class_matches(&graph.class_names[graph.object_class[v] as usize], class_filter))
        .map(|v| graph.class_names[graph.object_class[v] as usize].clone())
        .expect("matched non-empty");

    let instance_count = matched.len() as u64;
    let total_shallow = matched.iter().map(|(i, _)| i.shallow_bytes).sum();

    let mut instances: Vec<ExplainedInstance> =
        matched.iter().map(|(inst, _)| inst.clone()).collect();
    instances.sort_by(|a, b| b.shallow_bytes.cmp(&a.shallow_bytes));
    instances.truncate(top_instances);

    let mut retainer_counts: rustc_hash::FxHashMap<String, (u64, u64)> =
        rustc_hash::FxHashMap::default();
    for (_, hits) in &matched {
        for (rc, shallow) in hits {
            let entry = retainer_counts.entry(rc.clone()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += shallow;
        }
    }

    let mut retainer_rows: Vec<RetainerRow> = retainer_counts
        .into_iter()
        .map(|(retainer_class, (instance_count, shallow_bytes))| RetainerRow {
            retainer_class,
            instance_count,
            shallow_bytes,
        })
        .collect();
    retainer_rows.sort_by(|a, b| b.shallow_bytes.cmp(&a.shallow_bytes));
    retainer_rows.truncate(top_retainers);

    Some(ClassExplanation {
        class_name,
        instance_count,
        total_shallow,
        top_instances: instances,
        top_retainers: retainer_rows,
    })
}

/// Compute retained sizes for every object and aggregate them per class.
///
/// Builds the dominator tree via [`compute_dominators`], accumulates each
/// object's retained size bottom-up (deepest dominator-tree level first, in
/// parallel within each level), then aggregates per-class totals and reachable
/// / unreachable counts. The returned [`RetainedAnalysis`] has both the
/// per-class and per-object rows sorted by retained size descending.
///
/// Set `quiet` to `true` to suppress progress spinners.
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

    let max_depth = depth.iter().copied().max().unwrap_or(0);

    let mut retained = vec![0u64; n];
    for d in (0..=max_depth).rev() {
        let nodes_at_depth: Vec<usize> = (0..n).filter(|&v| depth[v] == d).collect();
        let updates: Vec<(usize, u64)> = nodes_at_depth
            .par_iter()
            .map(|&v| {
                let mut sum = graph.shallow[v] as u64;
                for &c in &dom_children[v] {
                    sum += retained[c as usize];
                }
                (v, sum)
            })
            .collect();
        for (v, sum) in updates {
            retained[v] = sum;
        }
        progress.add_nodes(nodes_at_depth.len() as u64);
    }
    progress.finish(format!(
        "Retained sizes for {} objects",
        format_count(n as u64)
    ));

    let progress = group.begin(3, "aggregating by class");
    let k = graph.class_names.len();

    let (class_count, class_shallow, class_retained, reachable, unreachable, total_shallow) =
        (0..n)
            .into_par_iter()
            .fold(
                || {
                    (
                        vec![0u64; k],
                        vec![0u64; k],
                        vec![0u64; k],
                        0u64,
                        0u64,
                        0u64,
                    )
                },
                |mut acc, v| {
                    acc.5 += graph.shallow[v] as u64;
                    let c = graph.object_class[v] as usize;
                    acc.0[c] += 1;
                    acc.1[c] += graph.shallow[v] as u64;
                    acc.2[c] += retained[v];
                    if depth[v] > 0 {
                        acc.3 += 1;
                    } else {
                        acc.4 += 1;
                    }
                    acc
                },
            )
            .reduce(
                || {
                    (
                        vec![0u64; k],
                        vec![0u64; k],
                        vec![0u64; k],
                        0u64,
                        0u64,
                        0u64,
                    )
                },
                |mut a, b| {
                    for i in 0..k {
                        a.0[i] += b.0[i];
                        a.1[i] += b.1[i];
                        a.2[i] += b.2[i];
                    }
                    a.3 += b.3;
                    a.4 += b.4;
                    a.5 += b.5;
                    a
                },
            );

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
        .into_par_iter()
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
        idom,
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
        assert_eq!(by_addr[&0x3001], 48);
        assert_eq!(by_addr[&0x3000], 72);
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
    fn explain_class_reports_retainers() {
        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = crate::graph::ObjectGraph::build(&hprof, &index, true).unwrap();
        let explanation = explain_class(&graph, "Node", 10, 10).unwrap();
        assert_eq!(explanation.instance_count, 3);
        assert!(explanation
            .top_retainers
            .iter()
            .any(|r| r.retainer_class == "GC root" || r.retainer_class.contains("Node")));
    }
}
