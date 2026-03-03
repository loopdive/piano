/// A single note in the song.
#[derive(Clone, Debug)]
pub struct Note {
    pub pitch: u8,
    pub start_time: f32,
    pub duration: f32,
    pub velocity: f32,
}

pub struct Song {
    pub notes: Vec<Note>,
    pub bpm: f32,
}

/// Convert MIDI note number (60 = C4) to pitch (0-87).
/// MIDI 21 = A0 (pitch 0), MIDI 108 = C8 (pitch 87).
pub fn midi_to_pitch(midi_note: u8) -> u8 {
    midi_note.saturating_sub(21)
}

/// Create a demo song: C major scale up/down + chords.
pub fn demo_song() -> Song {
    let bpm = 120.0;
    let beat = 60.0 / bpm;
    let mut notes = Vec::new();

    // C major scale ascending: C4 D4 E4 F4 G4 A4 B4 C5
    let scale_up = [60, 62, 64, 65, 67, 69, 71, 72];
    for (i, &midi) in scale_up.iter().enumerate() {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: i as f32 * beat,
            duration: beat * 0.9,
            velocity: 0.8,
        });
    }

    // C major scale descending: B4 A4 G4 F4 E4 D4 C4
    let scale_down = [71, 69, 67, 65, 64, 62, 60];
    let offset = 8.0 * beat;
    for (i, &midi) in scale_down.iter().enumerate() {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: offset + i as f32 * beat,
            duration: beat * 0.9,
            velocity: 0.8,
        });
    }

    // Chords section
    let chord_offset = 16.0 * beat;
    // C major chord (C4-E4-G4)
    for &midi in &[60, 64, 67] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // F major chord (F4-A4-C5)
    for &midi in &[65, 69, 72] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 2.0,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // G major chord (G4-B4-D5)
    for &midi in &[67, 71, 74] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 4.0,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // C major chord final (C4-E4-G4-C5)
    for &midi in &[60, 64, 67, 72] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 6.0,
            duration: beat * 4.0,
            velocity: 1.0,
        });
    }

    Song { notes, bpm }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_pitch() {
        assert_eq!(midi_to_pitch(21), 0);   // A0
        assert_eq!(midi_to_pitch(60), 39);  // C4
        assert_eq!(midi_to_pitch(108), 87); // C8
    }

    #[test]
    fn test_demo_song_has_notes() {
        let song = demo_song();
        assert!(!song.notes.is_empty());
        assert!(song.notes.len() > 20);
    }

    #[test]
    fn test_demo_song_pitches_in_range() {
        let song = demo_song();
        for note in &song.notes {
            assert!(note.pitch <= 87, "Pitch {} out of range", note.pitch);
        }
    }

    #[test]
    fn test_demo_song_times_positive() {
        let song = demo_song();
        for note in &song.notes {
            assert!(note.start_time >= 0.0);
            assert!(note.duration > 0.0);
        }
    }
}
