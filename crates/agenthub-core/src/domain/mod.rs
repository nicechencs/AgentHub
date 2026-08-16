//! Pure domain logic that is not wire DTO / persistence.
//!
//! Keep I/O and repository access in `services`; this tree holds planning
//! tables and other side-effect-free domain graphs.

pub mod protocol_graph;
