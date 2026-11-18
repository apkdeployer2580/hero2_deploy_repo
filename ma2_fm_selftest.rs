// Standalone self-test of hero2's MA-2 FM synth.
//
//   rustc --edition 2021 --test ma2_fm_selftest.rs -o tt_ma2_fm.exe && ./tt_ma2_fm.exe
//
// ★ This `include!`s the REAL source, not a copy — so the envelope tables, the voice decoding
// and the channel rules under test are exactly the ones the app ships.
// `wie_android/src/ma2_fm.rs` must therefore stay dependency-free (edition 2021, no let-chains).
// The songs themselves are exercised by `tools/ma2_render` (they are game audio and stay out
// of the repository).

include!("wie_android/src/ma2_fm.rs");
