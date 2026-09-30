//! conforms: traits-permission-mode-exhaustive
//! conforms: traits-no-adjudicating-method
//!
//! The operator-facing policy vocabulary of the charter's section 3.4, closed and
//! exhaustive. The mode is what the operator declared and the harness reads it.

use serde::{Deserialize, Serialize};

/// The operator's answer to a class of action: ask first, permit, or refuse.
///
/// Three modes and no fourth, because the question has three answers. The enum
/// is exhaustive so a fourth cannot arrive without every consumer's match
/// failing to compile, which is the loudness the charter asks for:
///
/// ```
/// use weaver_traits::PermissionMode;
/// fn name(mode: PermissionMode) -> &'static str {
///     match mode {
///         PermissionMode::Ask => "ask",
///         PermissionMode::Allow => "allow",
///         PermissionMode::Deny => "deny",
///     }
/// }
/// ```
///
/// The type carries no method that adjudicates anything: no `is_allowed`, no
/// `check`, no predicate of any kind. A helper here would be the first step
/// toward a floor type that decides, which the charter forbids. The named
/// candidates are pinned by the compile-fail doctests at the crate root.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Ask,
    Allow,
    Deny,
}
