/// Piano keyboard layout calculations for 88 keys (A0 to C8).
/// Pitch 0 = A0, Pitch 87 = C8.

/// Returns true if the given pitch (0-87) is a black key.
pub fn is_black_key(pitch: u8) -> bool {
    // Pitch 0 = A0. Shift so C=0: C=0, C#=1, D=2, D#=3, E=4, F=5, F#=6, G=7, G#=8, A=9, A#=10, B=11
    let note = (pitch + 9) % 12;
    matches!(note, 1 | 3 | 6 | 8 | 10)
}

/// Returns the x-position (left edge) and width of a key in pixels,
/// given the total keyboard width and the pitch (0-87).
pub fn key_rect(pitch: u8, total_width: f32) -> (f32, f32) {
    let white_key_width = total_width / 52.0;
    let black_key_width = white_key_width * 0.6;

    if is_black_key(pitch) {
        let white_index = white_key_index_before(pitch);
        let x = (white_index as f32 + 1.0) * white_key_width - black_key_width / 2.0;
        (x, black_key_width)
    } else {
        let white_index = count_white_keys_before(pitch);
        let x = white_index as f32 * white_key_width;
        (x, white_key_width)
    }
}

fn count_white_keys_before(pitch: u8) -> u32 {
    (0..pitch).filter(|&p| !is_black_key(p)).count() as u32
}

fn white_key_index_before(pitch: u8) -> u32 {
    let mut p = pitch - 1;
    while is_black_key(p) {
        p -= 1;
    }
    count_white_keys_before(p)
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
    fn test_white_key_count() {
        let white_count = (0..88).filter(|&p| !is_black_key(p)).count();
        assert_eq!(white_count, 52);
    }

    #[test]
    fn test_black_key_count() {
        let black_count = (0..88).filter(|&p| is_black_key(p)).count();
        assert_eq!(black_count, 36);
    }

    #[test]
    fn test_key_rect_first_key() {
        let (x, w) = key_rect(0, 1040.0);
        assert!((x - 0.0).abs() < 0.01);
        assert!((w - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_key_rect_black_key() {
        let (x, w) = key_rect(1, 1040.0);
        let white_w = 1040.0 / 52.0;
        let black_w = white_w * 0.6;
        let expected_x = 1.0 * white_w - black_w / 2.0;
        assert!((x - expected_x).abs() < 0.01);
        assert!((w - black_w).abs() < 0.01);
    }

    #[test]
    fn test_key_rect_last_key() {
        let (x, w) = key_rect(87, 1040.0);
        let white_w = 1040.0 / 52.0;
        let expected_x = 51.0 * white_w;
        assert!((x - expected_x).abs() < 0.01);
        assert!((w - white_w).abs() < 0.01);
    }
}
