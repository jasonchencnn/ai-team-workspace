//! Dump §3's matrix as text for the Q-09 device to diff against the frozen spec.
//!
//! This exists so the device compares the spec against **what the compiler
//! actually sees**, not against a re-parse of the source. An in-repo example that
//! only prints is acceptable; it is also the tripwire for a future edit that
//! reorders `PermissionAction::ALL` without reordering `MATRIX`.

use teamspace_domain::permission_matrix::{ActorKind, MATRIX, PermissionAction, matrix_verdict};

fn main() {
    println!("#ACTOR\t{}", ActorKind::ALL.len());
    for a in ActorKind::ALL {
        println!("#COLUMN\t{}\t{}", a.index(), a.column_name());
    }
    println!("#ROW\t{}", PermissionAction::ALL.len());
    for action in PermissionAction::ALL {
        println!("#LABEL\t{}\t{}", action.index(), action.spec_label());
        for actor in ActorKind::ALL {
            println!(
                "CELL\t{}\t{}\t{}",
                action.index(),
                actor.index(),
                matrix_verdict(action, actor).spec_symbol()
            );
        }
    }
    println!("#MATRIX_LEN\t{}", MATRIX.len());
    println!(
        "#CONTEXT_DEPENDENT\t{}",
        MATRIX
            .iter()
            .flatten()
            .filter(|v| v.is_context_dependent())
            .count()
    );
}
