//! conforms: traits-compiles-on-stable
//! conforms: traits-data-derive-set
//! conforms: traits-derive-set-pinned-by-doctest
//! conforms: traits-no-default
//! conforms: traits-non-exhaustive-pinned-by-doctest
//! conforms: traits-adjudicators-pinned-by-doctest
//! conforms: traits-tool-no-safety-classification
//!
//! The floor's shared vocabulary: provider-agnostic conversation content and the
//! operator-facing permission modes. Every type here is data, derives what data
//! derives, and owns no invariant it cannot state in its own shape or name a party
//! to enforce, per `weaver-traits-Spec` section 2. This crate uses no nightly
//! feature and compiles on stable.
//!
//! The doctests below are the compile-time instruments of `weaver-traits-Spec`
//! section 7. Each pin names the assertion it enforces.
//!
//! The derive set is pinned by construction (`traits-derive-set-pinned-by-doctest`):
//! a function bounded on the five data derives accepts every message-model type and
//! the mode enum, so a derive dropped later stops the build.
//!
//! ```
//! fn is_data<T: std::fmt::Debug + Clone + PartialEq
//!     + serde::Serialize + for<'de> serde::Deserialize<'de>>() {}
//! is_data::<weaver_traits::Message>();
//! is_data::<weaver_traits::Role>();
//! is_data::<weaver_traits::ContentBlock>();
//! is_data::<weaver_traits::ToolCall>();
//! is_data::<weaver_traits::ToolResultBlock>();
//! is_data::<weaver_traits::PermissionMode>();
//! ```
//!
//! No `Default` on any type here (`traits-no-default`): absence is never read as a
//! default unless a charter says so, per `weaver-types-PRD` section 5.
//!
//! ```compile_fail
//! fn has_default<T: Default>() {}
//! has_default::<weaver_traits::Message>();
//! ```
//!
//! ```compile_fail
//! fn has_default<T: Default>() {}
//! has_default::<weaver_traits::PermissionMode>();
//! ```
//!
//! No adjudicating method on `PermissionMode` (`traits-adjudicators-pinned-by-doctest`):
//! the named candidates fail to compile, and the open-set prohibition stays review's,
//! per the split `weaver-traits-Spec` section 7 makes.
//!
//! ```compile_fail
//! let _ = weaver_traits::PermissionMode::Ask.is_allowed();
//! ```
//!
//! ```compile_fail
//! let _ = weaver_traits::PermissionMode::Ask.check();
//! ```
//!
//! The non-exhaustive elections are pinned from the outside
//! (`traits-non-exhaustive-pinned-by-doctest`): a match on `Role` or `ContentBlock`
//! without a wildcard arm fails to compile in a consumer crate, which is
//! `#[non_exhaustive]` doing its work where a consumer would feel it.
//!
//! ```compile_fail
//! fn f(r: weaver_traits::Role) -> u8 {
//!     match r {
//!         weaver_traits::Role::User => 0,
//!         weaver_traits::Role::Assistant => 1,
//!         weaver_traits::Role::ToolResult => 2,
//!     }
//! }
//! ```
//!
//! ```compile_fail
//! fn f(b: weaver_traits::ContentBlock) -> u8 {
//!     match b {
//!         weaver_traits::ContentBlock::Text { .. } => 0,
//!         weaver_traits::ContentBlock::ToolCall(_) => 1,
//!         weaver_traits::ContentBlock::ToolResult(_) => 2,
//!     }
//! }
//! ```
//!
//! The tool trait carries no safety classification
//! (`traits-tool-no-safety-classification`), per `weaver-traits-PRD` section
//! 3.1, which the tool workflow's chartering did not weaken. The previous
//! form of this pin asserted that nothing answered to the trait's name while
//! it was blocked, and promised this replacement when the workflow chartered
//! it: the flagship judgment method of the prior tree must not exist, and
//! the pin fires the day someone adds one.
//!
//! ```compile_fail
//! fn judged(tool: &dyn weaver_traits::Tool) {
//!     let _ = tool.invocation_properties("input");
//! }
//! ```

mod message;
mod permission;
mod provider;
mod tool;

pub use message::{ContentBlock, Message, Role, ToolCall, ToolResultBlock};
pub use permission::PermissionMode;
pub use tool::{Tool, ToolFailure};
