//! Keeping local undo history from reverting other people's changes.

use crate::ops::patch_node;
use crate::Op;
use desigma_doc::{Command, History};

/// Applies a remote patch to the nodes stored in `history`'s inverse commands (`Update`, and the
/// subtrees of `Insert` / `InsertPage` that undoing a delete restores, also inside `Batch`).
/// Undoing an earlier local edit then does not bring back values someone else has since changed.
/// Ops other than `Patch` are ignored. Call it for each op in `Received::applied`.
pub fn rebase_history(history: &mut History, op: &Op) {
    let Op::Patch { id, fields } = op else { return };
    history.for_each_command_mut(|c| match c {
        Command::Update { node } if node.id == *id => {
            let _ = patch_node(node, fields);
        }
        Command::Insert { nodes, .. } | Command::InsertPage { nodes, .. } => {
            for n in nodes.iter_mut().filter(|n| n.id == *id) {
                let _ = patch_node(n, fields);
            }
        }
        _ => {}
    });
}
