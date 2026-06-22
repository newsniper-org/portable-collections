//! Ordered byte-radix trie — the non-B+tree core.
//!
//! Properties this demonstrates (the synthesis claims):
//! * **No structural rebalancing (no SMOs).** Inserts only ever *add* a node;
//!   nothing rotates, splits-and-propagates, or merges. There is no balance
//!   invariant to restore, so there is no helping cascade — the root cause that
//!   makes lock-free *and* bounded-latency hard for comparison B+trees.
//! * **Constant depth.** Keys are fixed width (`KEY_LEN`), so every present key
//!   is at depth `KEY_LEN`; lookups take a constant number of hops regardless of
//!   the number of keys — the structural basis of the bounded-worst-case claim.
//! * **Ordered.** Children are kept byte-sorted, so in-order traversal yields
//!   keys in lexicographic = logical order -> efficient range scans.
//!
//! Node children use a sorted `Vec<(u8, Box<Node>)>` ("adaptive small node").
//! A production design (ART) would promote a hot node to a dense `[_; 256]`
//! array; that is a constant-factor optimization that does not change any
//! property under test, so it is left as a documented optimization.

use crate::key::KEY_LEN;

#[derive(Debug)]
struct Node<V> {
    /// Byte-sorted children. Invariant: strictly increasing by the `u8` key.
    children: Vec<(u8, Box<Node<V>>)>,
    /// Present iff a key ends exactly at this node (depth == KEY_LEN in our use).
    value: Option<V>,
}

impl<V> Node<V> {
    fn new() -> Self {
        Node {
            children: Vec::new(),
            value: None,
        }
    }

    #[inline]
    fn child_index(&self, b: u8) -> Result<usize, usize> {
        self.children.binary_search_by_key(&b, |(c, _)| *c)
    }
}

#[derive(Debug)]
pub struct RadixTrie<V> {
    root: Node<V>,
    len: usize,
    nodes: usize,
}

impl<V: Clone> Default for RadixTrie<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: Clone> RadixTrie<V> {
    pub fn new() -> Self {
        RadixTrie {
            root: Node::new(),
            len: 0,
            nodes: 1,
        }
    }

    /// Number of stored keys.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Total allocated nodes (a proxy for memory / structural churn). Grows with
    /// distinct key *bytes*, never with rebalancing — there is no rebalancing.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes
    }

    /// Insert / overwrite. Returns the previous value if the key existed.
    pub fn insert(&mut self, key: &[u8], v: V) -> Option<V> {
        let mut node = &mut self.root;
        for &b in key {
            match node.child_index(b) {
                Ok(i) => node = &mut node.children[i].1,
                Err(i) => {
                    node.children.insert(i, (b, Box::new(Node::new())));
                    self.nodes += 1;
                    node = &mut node.children[i].1;
                }
            }
        }
        let prev = node.value.replace(v);
        if prev.is_none() {
            self.len += 1;
        }
        prev
    }

    /// Point lookup.
    pub fn get(&self, key: &[u8]) -> Option<&V> {
        let mut node = &self.root;
        for &b in key {
            match node.child_index(b) {
                Ok(i) => node = &node.children[i].1,
                Err(_) => return None,
            }
        }
        node.value.as_ref()
    }

    /// Point lookup that also reports the number of node hops taken — used by the
    /// simulator to demonstrate that read cost is bounded by `KEY_LEN`,
    /// independent of the number of keys (the bounded-latency property).
    pub fn get_with_steps(&self, key: &[u8]) -> (Option<&V>, u32) {
        let mut node = &self.root;
        let mut steps = 0u32;
        for &b in key {
            steps += 1;
            match node.child_index(b) {
                Ok(i) => node = &node.children[i].1,
                Err(_) => return (None, steps),
            }
        }
        (node.value.as_ref(), steps)
    }

    /// Ordered range scan over `[lo, hi]` (both inclusive), with prefix pruning.
    /// Output is sorted ascending by key.
    pub fn range_inclusive(&self, lo: &[u8], hi: &[u8]) -> Vec<(Vec<u8>, V)> {
        let mut out = Vec::new();
        let mut path = Vec::with_capacity(KEY_LEN);
        Self::collect(&self.root, &mut path, lo, hi, &mut out);
        out
    }

    fn collect(node: &Node<V>, path: &mut Vec<u8>, lo: &[u8], hi: &[u8], out: &mut Vec<(Vec<u8>, V)>) {
        if let Some(v) = &node.value {
            if path.as_slice() >= lo && path.as_slice() <= hi {
                out.push((path.clone(), v.clone()));
            }
        }
        for (b, child) in &node.children {
            path.push(*b);
            let len = path.len();
            // Prune whole subtrees whose prefix is already strictly outside
            // [lo, hi]. `lo`/`hi` are full-length keys; compare against prefixes.
            let lo_pre = &lo[..len.min(lo.len())];
            let hi_pre = &hi[..len.min(hi.len())];
            let prune = path.as_slice() < lo_pre || path.as_slice() > hi_pre;
            if !prune {
                Self::collect(child, path, lo, hi, out);
            }
            path.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key::encode;

    #[test]
    fn insert_get_overwrite() {
        let mut t: RadixTrie<u32> = RadixTrie::new();
        assert_eq!(t.insert(&encode(1, 0, 0), 10), None);
        assert_eq!(t.insert(&encode(1, 0, 0), 11), Some(10));
        assert_eq!(t.get(&encode(1, 0, 0)), Some(&11));
        assert_eq!(t.get(&encode(1, 0, 1)), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn bounded_depth_independent_of_size() {
        let mut t: RadixTrie<u32> = RadixTrie::new();
        for i in 0..5000u64 {
            t.insert(&encode(i, i * 7, (i % 3) as u32), i as u32);
        }
        // Every lookup costs exactly KEY_LEN hops, no matter how many keys.
        let (_, steps) = t.get_with_steps(&encode(4999, 4999 * 7, 1));
        assert_eq!(steps as usize, KEY_LEN);
    }

    #[test]
    fn ordered_range() {
        let mut t: RadixTrie<u32> = RadixTrie::new();
        for off in [50u64, 10, 30, 20, 40] {
            t.insert(&encode(1, off, 0), off as u32);
        }
        t.insert(&encode(2, 0, 0), 999); // different inode, must be excluded
        let lo = encode(1, 0, 0);
        let hi = encode(1, 100, u32::MAX);
        let got: Vec<u32> = t.range_inclusive(&lo, &hi).into_iter().map(|(_, v)| v).collect();
        assert_eq!(got, vec![10, 20, 30, 40, 50]); // sorted, inode-2 excluded
    }
}
