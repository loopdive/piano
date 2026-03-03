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

/// Create a demo song with melody, bass, and chords spanning the full keyboard range.
pub fn demo_song() -> Song {
    let bpm = 120.0;
    let beat = 60.0 / bpm;
    let mut notes = Vec::new();

    // === Section 1: Melody (C major scale up) with bass accompaniment ===
    let scale_up = [60, 62, 64, 65, 67, 69, 71, 72]; // C4 to C5
    for (i, &midi) in scale_up.iter().enumerate() {
        // Melody
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: i as f32 * beat,
            duration: beat * 0.9,
            velocity: 0.85,
        });
        // Bass octaves (C2-C3 range)
        if i % 2 == 0 {
            let bass = if i < 4 { 36 } else { 41 }; // C2 or F2
            notes.push(Note {
                pitch: midi_to_pitch(bass),
                start_time: i as f32 * beat,
                duration: beat * 1.8,
                velocity: 0.6,
            });
            notes.push(Note {
                pitch: midi_to_pitch(bass + 12), // octave above
                start_time: i as f32 * beat,
                duration: beat * 1.8,
                velocity: 0.5,
            });
        }
    }

    // === Section 2: Descending scale with high sparkle notes ===
    let scale_down = [71, 69, 67, 65, 64, 62, 60]; // B4 to C4
    let offset = 8.0 * beat;
    for (i, &midi) in scale_down.iter().enumerate() {
        // Melody descending
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: offset + i as f32 * beat,
            duration: beat * 0.9,
            velocity: 0.85,
        });
        // High sparkle notes (C6-C7 range)
        if i % 2 == 0 {
            notes.push(Note {
                pitch: midi_to_pitch(midi + 24), // 2 octaves up
                start_time: offset + i as f32 * beat + beat * 0.5,
                duration: beat * 0.3,
                velocity: 0.7,
            });
        }
        // Deep bass pedal
        if i == 0 || i == 4 {
            notes.push(Note {
                pitch: midi_to_pitch(36), // C2
                start_time: offset + i as f32 * beat,
                duration: beat * 3.5,
                velocity: 0.55,
            });
        }
    }

    // === Section 3: Full chords with bass ===
    let chord_offset = 16.0 * beat;

    // C major chord (C3-C4-E4-G4-C5)
    for &midi in &[48, 60, 64, 67, 72] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // Bass C2
    notes.push(Note {
        pitch: midi_to_pitch(36),
        start_time: chord_offset,
        duration: beat * 2.0,
        velocity: 0.65,
    });

    // F major chord (F3-F4-A4-C5)
    for &midi in &[53, 65, 69, 72] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 2.0,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    notes.push(Note {
        pitch: midi_to_pitch(41), // F2 bass
        start_time: chord_offset + beat * 2.0,
        duration: beat * 2.0,
        velocity: 0.65,
    });

    // G major chord (G3-G4-B4-D5) + high G6
    for &midi in &[55, 67, 71, 74] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 4.0,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    notes.push(Note {
        pitch: midi_to_pitch(43), // G2 bass
        start_time: chord_offset + beat * 4.0,
        duration: beat * 2.0,
        velocity: 0.65,
    });
    notes.push(Note {
        pitch: midi_to_pitch(91), // G6 sparkle
        start_time: chord_offset + beat * 5.0,
        duration: beat * 0.5,
        velocity: 0.7,
    });

    // === Section 4: Grand finale — wide C major spread ===
    // C2-C3-C4-E4-G4-C5-E5-G5-C6
    for &midi in &[36, 48, 60, 64, 67, 72, 76, 79, 84] {
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
