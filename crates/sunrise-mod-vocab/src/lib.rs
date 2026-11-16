//! The API vocabulary re-exported by `sunrise-mod-api`.
#![deny(missing_docs)]
#![deny(unsafe_code)]
#![deny(ambiguous_glob_reexports)]
extern crate self as mod_api;

use std::sync::{Arc, OnceLock};

use serde_json::{Map, Value};
use std::time::Duration;

mod generated_host {
    include!(concat!(env!("OUT_DIR"), "/mod_api_generated_host.rs"));
}

// no wire: type alias inherits the Wire policy of its tuple or Result target.
/// Result of accessing or decoding an attachment.
pub type AttachmentResult<T> = std::result::Result<T, AttachmentError>;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/mod_api_generated.rs"));
}

// The crate's own tests implement hooks with `#[mod_api::__subscriptions]`, which marks each
// through the generated `__sunrise_mark!`; a macro the generated code exports can't be named by
// absolute path inside its own crate, so a test build has this no-op instead.
#[cfg(test)]
macro_rules! __sunrise_mark {
    ($owner:literal $method:ident) => {};
}
#[cfg(test)]
pub(crate) use __sunrise_mark;

include!(concat!(env!("OUT_DIR"), "/mod_api_exports_root.rs"));
include!(concat!(env!("OUT_DIR"), "/wire_generated.rs"));
