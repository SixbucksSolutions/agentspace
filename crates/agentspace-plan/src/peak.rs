//! A story's peak: the width of its `after` graph (D74; docs/lease-api.md, Plan).
//!
//! The subagents of a story are numbered in the order the plan writes them. `after[j]` holds the
//! numbers of the subagents `j` starts after. The peak is the largest set of subagents none of
//! which starts after another, directly or through others. By Dilworth's theorem that is the fewest
//! chains that cover the subagents, which is their number less a maximum matching on the
//! transitive closure.

/// The transitive closure of the starts: `later[i][j]` is true when `j` starts after `i`, directly
/// or through others. Entries of `after` that name no subagent are left out, as no plan that
/// reaches here has one.
pub(crate) fn closure(after: &[Vec<usize>]) -> Vec<Vec<bool>> {
    let n = after.len();
    let mut later = vec![vec![false; n]; n];
    for (j, list) in after.iter().enumerate() {
        for &i in list {
            if let Some(cell) = later.get_mut(i).and_then(|row| row.get_mut(j)) {
                *cell = true;
            }
        }
    }
    // Warshall's algorithm: after step k, a path through any of the first k subagents is known.
    for k in 0..n {
        let Some(from_k) = later.get(k).cloned() else {
            continue;
        };
        for row in later.iter_mut() {
            if row.get(k).copied().unwrap_or(false) {
                for (cell, &through) in row.iter_mut().zip(&from_k) {
                    *cell |= through;
                }
            }
        }
    }
    later
}

/// Whether the starts form a cycle: some subagent starts after itself, directly or through others.
pub(crate) fn has_cycle(later: &[Vec<bool>]) -> bool {
    later
        .iter()
        .enumerate()
        .any(|(i, row)| row.get(i).copied().unwrap_or(false))
}

/// The width of an acyclic closure: the subagents less a maximum matching that pairs each `i` with
/// some `j` that starts after it (Kuhn's augmenting paths). Each matched pair joins two subagents
/// into one chain, so the matching's size is how many fewer chains than subagents cover them all.
pub(crate) fn width(later: &[Vec<bool>]) -> usize {
    let n = later.len();
    let mut matched_to: Vec<Option<usize>> = vec![None; n];
    let mut matched = 0usize;
    for i in 0..n {
        let mut seen = vec![false; n];
        if augment(i, later, &mut matched_to, &mut seen) {
            matched += 1;
        }
    }
    n.saturating_sub(matched)
}

/// Finds an augmenting path from `i`, and flips it into the matching if there is one. Its depth is
/// at most the number of subagents, since each step marks one more `j` as seen.
fn augment(
    i: usize,
    later: &[Vec<bool>],
    matched_to: &mut [Option<usize>],
    seen: &mut [bool],
) -> bool {
    let Some(row) = later.get(i) else {
        return false;
    };
    for (j, &edge) in row.iter().enumerate() {
        if !edge {
            continue;
        }
        match seen.get_mut(j) {
            Some(s) if !*s => *s = true,
            _ => continue,
        }
        let free = match matched_to.get(j).copied().flatten() {
            None => true,
            Some(other) => augment(other, later, matched_to, seen),
        };
        if free {
            if let Some(slot) = matched_to.get_mut(j) {
                *slot = Some(i);
            }
            return true;
        }
    }
    false
}

/// A story's peak from its starts, or `None` when they form a cycle.
pub(crate) fn peak(after: &[Vec<usize>]) -> Option<usize> {
    let later = closure(after);
    if has_cycle(&later) {
        None
    } else {
        Some(width(&later))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subagents_with_no_starts_all_run_at_once() {
        assert_eq!(peak(&[vec![], vec![], vec![], vec![]]), Some(4));
        assert_eq!(peak(&[vec![]]), Some(1));
        assert_eq!(peak(&[]), Some(0));
    }

    #[test]
    fn a_chain_runs_one_at_a_time() {
        assert_eq!(peak(&[vec![], vec![0], vec![1], vec![2]]), Some(1));
    }

    #[test]
    fn the_0203_plan_counts_3() {
        // implementer, test author, runner; reviewer after all three (lease-api.md, Plan).
        assert_eq!(peak(&[vec![], vec![], vec![], vec![0, 1, 2]]), Some(3));
    }

    #[test]
    fn a_diamond_counts_its_middle() {
        // b and c after a, d after b and c.
        assert_eq!(peak(&[vec![], vec![0], vec![0], vec![1, 2]]), Some(2));
    }

    #[test]
    fn starts_through_others_count_as_starts() {
        // c after b after a, and d after a: a=0, b=1, c=2, d=3. {b, d} and {c, d} are the largest
        // sets, so 2.
        assert_eq!(peak(&[vec![], vec![0], vec![1], vec![0]]), Some(2));
        // e after c only, where c is after b after a: e is after a through others, so {a, e} is
        // not a set that can run at once. a=0, b=1, c=2, e=3, plus f with no starts.
        assert_eq!(peak(&[vec![], vec![0], vec![1], vec![2], vec![]]), Some(2));
        // An hourglass: c and e after b, which is after a and d. Two chains cover it only through
        // b (a, b, c and d, b, e share b), so without the closure the count would be 3.
        // a=0, d=1, b=2, c=3, e=4.
        assert_eq!(
            peak(&[vec![], vec![], vec![0, 1], vec![2], vec![2]]),
            Some(2)
        );
    }

    #[test]
    fn a_largest_set_can_span_depths() {
        // Two chains a0<a1<a2 and b0<b1, and x after a2 and b0. The set {a2, b1} spans depths,
        // and so does {a1, b1}; the width is 2, and a greedy one-pass match would need augmenting.
        // a0=0 a1=1 a2=2 b0=3 b1=4 x=5
        assert_eq!(
            peak(&[vec![], vec![0], vec![1], vec![], vec![3], vec![2, 3]]),
            Some(2)
        );
        // Kuhn needs an augmenting path here: 0 can pair with 1 or 2, 1 only with 2.
        // 0=a, 1=b after a, 2=c after a and b, 3=d after a. Chains {a, d} and {b, c}: width 2.
        assert_eq!(peak(&[vec![], vec![0], vec![0, 1], vec![0]]), Some(2));
    }

    #[test]
    fn a_cycle_has_no_peak() {
        assert_eq!(peak(&[vec![0]]), None);
        assert_eq!(peak(&[vec![1], vec![0]]), None);
        assert_eq!(peak(&[vec![], vec![0, 3], vec![1], vec![2]]), None);
    }

    /// A deterministic generator, so the cross-check below needs no dependency and never varies.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0 >> 33
        }
    }

    /// The width by brute force: the largest subset in which no member reaches another.
    fn brute_width(later: &[Vec<bool>]) -> usize {
        let n = later.len();
        let mut best = 0;
        for set in 0u32..(1 << n) {
            let members: Vec<usize> = (0..n).filter(|i| set & (1 << i) != 0).collect();
            let free = members
                .iter()
                .all(|&i| members.iter().all(|&j| !later[i][j]));
            if free {
                best = best.max(members.len());
            }
        }
        best
    }

    #[test]
    fn the_width_matches_brute_force_on_many_acyclic_graphs() {
        let mut rng = Lcg(0x5eed);
        for _ in 0..3000 {
            let n = (rng.next() % 11) as usize;
            let density = rng.next() % 100;
            // Starts only point back to earlier subagents, so the graph is acyclic; the order is
            // then shuffled so the numbering says nothing about the starts.
            let mut order: Vec<usize> = (0..n).collect();
            for i in (1..n).rev() {
                let j = (rng.next() % (i as u64 + 1)) as usize;
                order.swap(i, j);
            }
            let mut after = vec![Vec::new(); n];
            for j in 0..n {
                for i in 0..j {
                    if rng.next() % 100 < density {
                        after[order[j]].push(order[i]);
                    }
                }
            }
            let later = closure(&after);
            assert!(!has_cycle(&later));
            assert_eq!(width(&later), brute_width(&later), "after {after:?}");
        }
    }

    #[test]
    fn the_largest_story_counts_without_trouble() {
        // 32 subagents with no starts, and 32 in one chain.
        assert_eq!(peak(&vec![Vec::new(); 32]), Some(32));
        let chain: Vec<Vec<usize>> = (0..32)
            .map(|i| if i == 0 { vec![] } else { vec![i - 1] })
            .collect();
        assert_eq!(peak(&chain), Some(1));
    }
}
