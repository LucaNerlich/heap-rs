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
    let mut vertex = vec![0u32; total];
    let mut label = (0..total as u32).collect::<Vec<_>>();
    let mut idom = vec![0u32; total];
    let mut bucket: Vec<Vec<u32>> = vec![Vec::new(); total];
    let mut ancestor = vec![0u32; total];
    let mut dfnum = vec![0i32; total];
    let mut next_df = 0i32;

    // Iterative DFS from super-root
    let mut stack: Vec<(u32, usize)> = vec![(graph.super_root, 0)];
    dfnum[root] = next_df;
    next_df += 1;
    semi[root] = graph.super_root;
    vertex[0] = graph.super_root;

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
            if dfnum[w as usize] == -1 {
                parent[w as usize] = v;
                dfnum[w as usize] = next_df;
                next_df += 1;
                semi[w as usize] = w;
                vertex[(dfnum[w as usize] - 1) as usize] = w;
                stack.push((w, 0));
            }
        }
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

    fn link(v: u32, w: u32, ancestor: &mut [u32], label: &mut [u32], dfnum: &[i32]) {
        let mut s = w;
        loop {
            compress(s, ancestor, label, dfnum);
            if dfnum[label[s as usize] as usize] >= dfnum[label[v as usize] as usize] {
                label[s as usize] = v;
            }
            if ancestor[s as usize] == 0 {
                ancestor[s as usize] = v;
                break;
            }
            s = ancestor[s as usize];
        }
    }

    for i in (1..next_df as usize).rev() {
        let w = vertex[i];
        let mut best = w;
        for &v in &preds[w as usize] {
            if dfnum[v as usize] == -1 {
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
        for &v in &bucket[p as usize] {
            let y = eval(v, &mut ancestor, &mut label, &dfnum);
            idom[v as usize] = if semi[y as usize] < semi[v as usize] {
                y
            } else {
                p
            };
        }
        bucket[p as usize].clear();
    }

    for i in 1..next_df as usize {
        let w = vertex[i];
        if idom[w as usize] != semi[w as usize] {
            idom[w as usize] = idom[idom[w as usize] as usize];
        }
    }

    idom[root] = root as u32;

    idom
}
