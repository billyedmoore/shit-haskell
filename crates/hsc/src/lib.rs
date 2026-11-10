//! The hsc library: the front end, the Core-to-code translation and the runtime. The `hsc`
//! binary is the driver; an output program links this library (as a static library) with the
//! object code generated for it.

pub mod core;
pub mod driver;
pub mod interp;
pub mod native;
pub mod rename;
pub mod syntax;
pub mod tc;
