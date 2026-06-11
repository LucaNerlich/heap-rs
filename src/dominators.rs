use crate::graph::ObjectGraph;

/// Compute immediate dominators using Lengauer-Tarjan with an iterative DFS.
/// Returns idom[v] for each node v in 0..=super_root (inclusive).
pub fn compute_dominators(graph: &ObjectGraph) -> Vec<u32> {
    let n = graph.num_nodes;
    let root = graph.super_root as usize;
    let total = n + 1;

    let mut preds: Vec<Vec<u32>> = vec![Vec::new(); total];
    for v in 0..n {
        let start = graph.offsets[v] as usize;
        let end = graph.offsets[v + 1] as usize;
        for &w in &graph.targets[start..end] {
            preds[w as usize].push(v as u32);
        }
    }
    for &r in &graph.roots {
        preds[r as usize].push(graph.super_root);
    }

    let mut parent = vec![0u32; total];
    let mut semi = vec![0u32; total];
    // 1-based DFS numbering; vertex[0] is unused.
    let mut vertex = vec![0u32; total + 1];
    let mut label = (0..total as u32).collect::<Vec<_>>();
    let mut idom = vec![0u32; total];
    let mut bucket: Vec<Vec<u32>> = vec![Vec::new(); total];
    let mut ancestor = vec![0u32; total];
    let mut dfnum = vec![0i32; total];
    let mut next_df = 1i32;

    // Iterative DFS from super-root (dfnum is 1-based; 0 means unvisited).
    let mut stack: Vec<(u32, usize)> = vec![(graph.super_root, 0)];
    dfnum[root] = next_df;
    next_df += 1;
    semi[root] = graph.super_root;
    vertex[1] = graph.super_root;

    while let Some((v, child_idx)) = stack.pop() {
        let children = if v == graph.super_root {
            graph.roots.clone()
        } else {
            let start = graph.offsets[v as usize] as usize;
            let end = graph.offsets[v as usize + 1] as usize;
            graph.targets[start..end].to_vec()
        };

        if child_idx < children.len() {
            stack.push((v, child_idx + 1));
            let w = children[child_idx];
            if dfnum[w as usize] == 0 {
                parent[w as usize] = v;
                dfnum[w as usize] = next_df;
                next_df += 1;
                semi[w as usize] = w;
                vertex[dfnum[w as usize] as usize] = w;
                stack.push((w, 0));
            }
        }
    }

    fn link(v: u32, w: u32, ancestor: &mut [u32], label: &mut [u32]) {
        ancestor[w as usize] = v;
        label[w as usize] = w;
    }

    fn compress(v: u32, ancestor: &mut [u32], label: &mut [u32], dfnum: &[i32]) {
        if ancestor[v as usize] != 0 {
            compress(ancestor[v as usize], ancestor, label, dfnum);
            if dfnum[label[ancestor[v as usize] as usize] as usize]
                < dfnum[label[v as usize] as usize]
            {
                label[v as usize] = label[ancestor[v as usize] as usize];
            }
            ancestor[v as usize] = ancestor[ancestor[v as usize] as usize];
        }
    }

    fn eval(v: u32, ancestor: &mut [u32], label: &mut [u32], dfnum: &[i32]) -> u32 {
        if ancestor[v as usize] == 0 {
            return label[v as usize];
        }
        compress(v, ancestor, label, dfnum);
        if dfnum[label[ancestor[v as usize] as usize] as usize]
            < dfnum[label[v as usize] as usize]
        {
            label[v as usize]
        } else {
            label[ancestor[v as usize] as usize]
        }
    }

    for i in (2..next_df as usize).rev() {
        let w = vertex[i];
        let mut best = w;
        for &v in &preds[w as usize] {
            if dfnum[v as usize] == 0 {
                continue;
            }
            let y = eval(v, &mut ancestor, &mut label, &dfnum);
            if dfnum[semi[y as usize] as usize] < dfnum[semi[best as usize] as usize] {
                best = semi[y as usize];
            }
        }
        semi[w as usize] = best;
        bucket[best as usize].push(w);

        let p = parent[w as usize];
        link(p, w, &mut ancestor, &mut label);
        for &v in &bucket[p as usize] {
            let y = eval(v, &mut ancestor, &mut label, &dfnum);
            idom[v as usize] = if dfnum[semi[y as usize] as usize] < dfnum[semi[v as usize] as usize] {
                y
            } else {
                p
            };
        }
        bucket[p as usize].clear();
    }

    for i in 2..next_df as usize {
        let w = vertex[i];
        if semi[w as usize] != w && idom[w as usize] != semi[w as usize] {
            idom[w as usize] = idom[idom[w as usize] as usize];
        }
    }

    idom[root] = graph.super_root;

    idom
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::ObjectGraph;

    #[test]
    fn dominator_output_has_entry_for_every_node_plus_super_root() {
        let graph = ObjectGraph {
            addrs: vec![0xA, 0xB, 0xC],
            shallow: vec![10, 20, 30],
            class_names: vec!["Node".into()],
            object_class: vec![0, 0, 0],
            offsets: vec![0, 1, 2, 2],
            targets: vec![1, 2],
            roots: vec![0],
            num_nodes: 3,
            super_root: 3,
        };
        let idom = compute_dominators(&graph);
        assert_eq!(idom.len(), 4);
        assert_eq!(idom[3], 3);
        assert_eq!(idom[0], 3);
        assert_eq!(idom[1], 0);
        assert_eq!(idom[2], 1);
    }

    #[test]
    fn dominators_for_linked_list_fixture() {
        use crate::testutil::hprof::OwnedFixture;

        let fixture = OwnedFixture::linked_list();
        let hprof = fixture.parse();
        let index = crate::index::HeapIndex::build(&hprof, true).unwrap();
        let graph = ObjectGraph::build(&hprof, &index, true).unwrap();
        let idom = compute_dominators(&graph);
        assert_eq!(idom.len(), graph.num_nodes + 1);
        assert_eq!(idom[graph.super_root as usize], graph.super_root);
        assert_eq!(idom[0], graph.super_root);
        assert_eq!(idom[1], 0);
        assert_eq!(idom[2], 1);
    }
}
