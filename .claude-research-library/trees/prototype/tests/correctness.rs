//! End-to-end tests over the public API: snapshot visibility, range scans,
//! crash recovery, and a full simulation run.

use radix_fs_prototype::sim::{run, Config};
use radix_fs_prototype::store::{replay, FsCore, Value};

#[test]
fn snapshot_visibility_and_overwrite() {
    let mut fs = FsCore::new();
    let root = fs.snaps.root();

    // Write in root, then snapshot it.
    fs.put(1, 0, root, Value::Extent(100, 4));
    let child = fs.create_snapshot(root);

    // Child inherits the root value.
    assert_eq!(fs.get(1, 0, child), Some(Value::Extent(100, 4)));
    assert_eq!(fs.get(1, 0, root), Some(Value::Extent(100, 4)));

    // Overwrite in the child: root must be unchanged (snapshot isolation).
    fs.put(1, 0, child, Value::Extent(200, 8));
    assert_eq!(fs.get(1, 0, child), Some(Value::Extent(200, 8)));
    assert_eq!(fs.get(1, 0, root), Some(Value::Extent(100, 4)));

    // Tombstone in the child hides the key only in the child.
    fs.delete(1, 0, child);
    assert_eq!(fs.get(1, 0, child), None);
    assert_eq!(fs.get(1, 0, root), Some(Value::Extent(100, 4)));
}

#[test]
fn range_scan_is_ordered_and_snapshot_consistent() {
    let mut fs = FsCore::new();
    let root = fs.snaps.root();
    for off in [5u64, 1, 9, 3, 7] {
        fs.put(1, off, root, Value::Extent(off, 1));
    }
    let child = fs.create_snapshot(root);
    fs.delete(1, 5, child); // hide offset 5 in child
    fs.put(1, 4, child, Value::Extent(444, 1)); // add offset 4 in child

    let at_root: Vec<u64> = fs.range(1, 0, 100, root).into_iter().map(|(o, _)| o).collect();
    assert_eq!(at_root, vec![1, 3, 5, 7, 9]); // ordered

    let at_child: Vec<u64> = fs.range(1, 0, 100, child).into_iter().map(|(o, _)| o).collect();
    assert_eq!(at_child, vec![1, 3, 4, 7, 9]); // 5 hidden, 4 added, still ordered
}

#[test]
fn crash_recovery_matches_durable_prefix() {
    let mut fs = FsCore::new();
    let root = fs.snaps.root();
    for i in 0..200u64 {
        fs.put(i % 10, i, root, Value::Inode(i));
    }
    // Full replay reproduces the live state.
    let recovered = replay(fs.journal.ops());
    for i in 0..200u64 {
        assert_eq!(recovered.get(i % 10, i, root), fs.get(i % 10, i, root));
    }
    // A torn-tail prefix reproduces exactly the prefix's effect.
    let cut = 137;
    let prefix = fs.journal.prefix(cut);
    let partial = replay(&prefix);
    // Only the first `cut` puts are durable.
    assert_eq!(partial.get(6, 136, root), Some(Value::Inode(136)));
    assert_eq!(partial.get(7, 137, root), None); // op #137 (0-indexed) lost
}

#[test]
fn full_simulation_holds_all_invariants() {
    let cfg = Config {
        seed: 0xABCD,
        steps: 12_000,
        inodes: 48,
        offsets: 48,
        crash_every: 1_500,
    };
    let rep = run(&cfg);
    assert!(rep.ok(), "simulation invariants failed: {:?}", rep);
}
