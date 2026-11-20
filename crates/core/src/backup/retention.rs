//! Retenção: mantém as `keep` versões mais recentes não fixadas, mais todas as fixadas.

use super::library::SnapshotRef;

/// Stems das versões a apagar.
pub fn to_delete(refs: &[SnapshotRef], keep: usize) -> Vec<String> {
    let mut unpinned: Vec<&SnapshotRef> = refs.iter().filter(|r| !r.pinned).collect();
    unpinned.sort_by_key(|r| std::cmp::Reverse(r.created));
    unpinned.iter().skip(keep).map(|r| r.stem.clone()).collect()
}
