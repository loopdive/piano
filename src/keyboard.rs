/// Piano keyboard layout calculations.
/// Pitch 0 = A0, Pitch 87 = C8.
/// Visible range: 7 octaves C1–B7 (pitches 3–86).

/// First visible pitch (C1)
pub const VISIBLE_START: u8 = 3;
/// Last visible pitch (B7)
pub const VISIBLE_END: u8 = 86;

/// Returns true if the given pitch (0-87) is a black key.
pub fn is_black_key(pitch: u8) -> bool {
    // Pitch 0 = A0. Shift so C=0: C=0, C#=1, D=2, D#=3, E=4, F=5, F#=6, G=7, G#=8, A=9, A#=10, B=11
    let note = (pitch + 9) % 12;
    matches!(note, 1 | 3 | 6 | 8 | 10)
}

/// Number of white keys in the visible range
fn visible_white_count() -> u32 {
    (VISIBLE_START..=VISIBLE_END)
        .filter(|&p| !is_black_key(p))
        .count() as u32
}

/// Returns the x-position (left edge) and width of a key in pixels,
/// given the total keyboard width and the pitch (0-87).
/// Keys outside the visible range are clamped to the edges.
pub fn key_rect(pitch: u8, total_width: f32) -> (f32, f32) {
    let white_count = visible_white_count();
    let white_key_width = total_width / white_count as f32;
    let black_key_width = white_key_width * 0.6;

    if is_black_key(pitch) {
        let white_index = white_key_index_before(pitch);
        let x = (white_index as f32 + 1.0) * white_key_width - black_key_width / 2.0;
        (x, black_key_width)
    } else {
        let white_index = count_white_keys_in_range(VISIBLE_START, pitch);
        let x = white_index as f32 * white_key_width;
        (x, white_key_width)
    }
}

/// Returns true if the pitch is in the visible keyboard range.
pub fn is_visible(pitch: u8) -> bool {
    pitch >= VISIBLE_START && pitch <= VISIBLE_END
}

fn count_white_keys_in_range(from: u8, to: u8) -> u32 {
    (from..to).filter(|&p| !is_black_key(p)).count() as u32
}

fn white_key_index_before(pitch: u8) -> u32 {
    let mut p = pitch - 1;
    while is_black_key(p) {
        p -= 1;
    }
    count_white_keys_in_range(VISIBLE_START, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_black_key() {
        assert!(!is_black_key(0));  // A0 = white
        assert!(is_black_key(1));   // A#0 = black
        assert!(!is_black_key(2));  // B0 = white
        assert!(!is_black_key(3));  // C1 = white
        assert!(is_black_key(4));   // C#1 = black
        assert!(!is_black_key(5));  // D1 = white
        assert!(!is_black_key(87)); // C8 = white
    }

    #[test]
    fn test_visible_white_key_count() {
        // 7 octaves (C1-B7) = 7 * 7 = 49 white keys
        assert_eq!(visible_white_count(), 49);
    }

    #[test]
    fn test_visible_range() {
        assert!(!is_visible(0));  // A0 - below range
        assert!(!is_visible(2));  // B0 - below range
        assert!(is_visible(3));   // C1 - start
        assert!(is_visible(86));  // B7 - end
        assert!(!is_visible(87)); // C8 - above range
    }

    #[test]
    fn test_key_rect_first_visible() {
        // C1 = pitch 3, first visible white key
        let (x, _w) = key_rect(3, 980.0);
        assert!((x - 0.0).abs() < 0.01);
        let white_w = 980.0 / 49.0;
        assert!((_w - white_w).abs() < 0.01);
    }

    #[test]
    fn test_key_rect_black_key() {
        // C#1 = pitch 4, first visible black key
        let white_w = 980.0 / 49.0;
        let black_w = white_w * 0.6;
        let (x, w) = key_rect(4, 980.0);
        let expected_x = 1.0 * white_w - black_w / 2.0;
        assert!((x - expected_x).abs() < 0.01);
        assert!((w - black_w).abs() < 0.01);
    }
}
