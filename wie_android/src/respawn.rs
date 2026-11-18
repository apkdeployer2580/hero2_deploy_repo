use std::{
    path::Path,
    sync::atomic::{AtomicU8, Ordering},
};
static RATE: AtomicU8 = AtomicU8::new(1);
pub const RATES: [u8; 6] = [1, 2, 3, 4, 5, 6];
pub fn rate() -> u8 {
    RATE.load(Ordering::Relaxed)
}
pub fn load(dir: &Path) {
    let value = std::fs::read_to_string(dir.join("wie_respawn_rate.txt"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|v| RATES.contains(v))
        .unwrap_or(1);
    RATE.store(value, Ordering::Relaxed);
}
pub fn cycle(dir: &Path, delta: i8) {
    let i = RATES.iter().position(|v| *v == rate()).unwrap_or(0) as i32;
    let value = RATES[(i + i32::from(delta)).rem_euclid(6) as usize];
    RATE.store(value, Ordering::Relaxed);
    if let Err(e) = std::fs::write(dir.join("wie_respawn_rate.txt"), value.to_string()) {
        log::warn!("respawn setting save: {e}");
    }
}
pub fn patch(current: [u8; 2], rate: u8) -> Option<[u8; 2]> {
    if current[1] != 0x3e || !RATES.contains(&current[0]) || !RATES.contains(&rate) {
        return None;
    }
    (current[0] != rate).then_some([rate, 0x3e])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_is_guarded_and_restorable() {
        assert_eq!(patch([1, 0x3e], 6), Some([6, 0x3e]));
        assert_eq!(patch([6, 0x3e], 1), Some([1, 0x3e]));
        assert_eq!(patch([6, 0x3e], 6), None);
        assert_eq!(patch([1, 0x3f], 6), None);
    }
}
