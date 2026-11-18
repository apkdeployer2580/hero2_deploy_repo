extern crate self as log;
#[macro_export] macro_rules! warn { ($($arg:tt)*) => { eprintln!($($arg)*) }; }
include!("wie_android/src/respawn.rs");
