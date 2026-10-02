//! Read-only access to the files of a user's own World at War install.
//!
//! Nothing here contains game data: every byte comes from the install at
//! runtime. The modules are deliberately engine-independent so they can be
//! unit tested and reused by the command-line tools in `examples/`.

pub mod install;
pub mod iwd;
pub mod iwi;
pub mod mapents;
pub mod t4;
pub mod zombiemap;
pub mod zone;

pub use install::Install;
pub use iwd::Iwd;
