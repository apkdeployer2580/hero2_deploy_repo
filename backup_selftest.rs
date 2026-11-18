// Standalone self-test of the host-driven recovery backup.
//
//   rustc --edition 2021 --test backup_selftest.rs -o tt_backup.exe && ./tt_backup.exe
//
// ★ This `include!`s the REAL source, so the frame shapes and the handle walk under
// test are exactly the ones the app ships. `wie_android/src/backup.rs` must therefore
// stay dependency-free (same contract as endpoint.rs / buffs.rs; see CLAUDE.md).
//
// Why standalone: `cargo test -p wie_android` dies at startup on this Windows host
// because of the linked winit/android deps.

include!("wie_android/src/backup.rs");
