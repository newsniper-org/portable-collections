//! Filesystem key encoding.
//!
//! A key is `(inode, offset, snapshot)` encoded as a **fixed-width,
//! big-endian, binary-comparable** byte string so that radix (lexicographic)
//! order is exactly the logical order `(inode, offset, snapshot)`. Fixed width
//! is what makes the trie depth a **constant** (`KEY_LEN` hops) — the structural
//! basis of the bounded-worst-case (soft-realtime) read claim.

pub type Inode = u64;
pub type Offset = u64;
pub type SnapId = u32;

/// 8 (inode) + 8 (offset) + 4 (snapshot) = 20 bytes. Constant trie depth.
pub const KEY_LEN: usize = 20;

#[inline]
pub fn encode(inode: Inode, offset: Offset, snap: SnapId) -> [u8; KEY_LEN] {
    let mut k = [0u8; KEY_LEN];
    k[0..8].copy_from_slice(&inode.to_be_bytes());
    k[8..16].copy_from_slice(&offset.to_be_bytes());
    k[16..20].copy_from_slice(&snap.to_be_bytes());
    k
}

#[inline]
pub fn decode(k: &[u8]) -> (Inode, Offset, SnapId) {
    debug_assert_eq!(k.len(), KEY_LEN);
    let inode = u64::from_be_bytes(k[0..8].try_into().unwrap());
    let offset = u64::from_be_bytes(k[8..16].try_into().unwrap());
    let snap = u32::from_be_bytes(k[16..20].try_into().unwrap());
    (inode, offset, snap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let k = encode(42, 4096, 7);
        assert_eq!(decode(&k), (42, 4096, 7));
    }

    #[test]
    fn radix_order_is_logical_order() {
        // (inode, offset, snap) ascending must be byte-ascending.
        let a = encode(1, 0, 0);
        let b = encode(1, 0, 1);
        let c = encode(1, 1, 0);
        let d = encode(2, 0, 0);
        assert!(a < b && b < c && c < d);
    }
}
