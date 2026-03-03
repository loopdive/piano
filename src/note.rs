use alloc::collections::BTreeMap;
use alloc::vec::Vec;

extern crate alloc;

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

/// Parse a standard MIDI file into a Song.
pub fn parse_midi(data: &[u8]) -> Result<Song, &'static str> {
    use midly::{MidiMessage, MetaMessage, Smf, Timing, TrackEventKind};

    let smf = Smf::parse(data).map_err(|_| "Failed to parse MIDI")?;

    let ticks_per_beat = match smf.header.timing {
        Timing::Metrical(tpb) => tpb.as_int() as f64,
        Timing::Timecode(fps, sub) => (fps.as_f32() * sub as f32) as f64,
    };

    // Build tempo map: (absolute_tick, microseconds_per_beat)
    let mut tempo_map: Vec<(u64, u32)> = Vec::new();
    for track in &smf.tracks {
        let mut abs_tick: u64 = 0;
        for event in track {
            abs_tick += event.delta.as_int() as u64;
            if let TrackEventKind::Meta(MetaMessage::Tempo(tempo)) = event.kind {
                tempo_map.push((abs_tick, tempo.as_int()));
            }
        }
    }
    tempo_map.sort_by_key(|&(tick, _)| tick);
    if tempo_map.is_empty() || tempo_map[0].0 != 0 {
        tempo_map.insert(0, (0, 500_000)); // default 120 BPM
    }

    let bpm = 60_000_000.0 / tempo_map[0].1 as f32;

    // Convert ticks to seconds
    let ticks_to_secs = |tick: u64| -> f64 {
        let mut secs = 0.0_f64;
        let mut prev_tick: u64 = 0;
        let mut us_per_beat: u32 = 500_000;
        for &(change_tick, new_tempo) in &tempo_map {
            if change_tick >= tick { break; }
            let delta = change_tick - prev_tick;
            secs += (delta as f64) * (us_per_beat as f64) / (ticks_per_beat * 1_000_000.0);
            prev_tick = change_tick;
            us_per_beat = new_tempo;
        }
        let remaining = tick - prev_tick;
        secs += (remaining as f64) * (us_per_beat as f64) / (ticks_per_beat * 1_000_000.0);
        secs
    };

    let mut notes = Vec::new();

    for track in &smf.tracks {
        // active notes: (channel, pitch) -> (velocity, start_tick)
        let mut active: BTreeMap<(u8, u8), (u8, u64)> = BTreeMap::new();
        let mut abs_tick: u64 = 0;

        for event in track {
            abs_tick += event.delta.as_int() as u64;
            if let TrackEventKind::Midi { channel, message } = event.kind {
                let ch = channel.as_int();
                match message {
                    MidiMessage::NoteOn { key, vel } => {
                        let pitch = key.as_int();
                        let velocity = vel.as_int();
                        if velocity == 0 {
                            // NoteOn vel=0 == NoteOff
                            if let Some((on_vel, start)) = active.remove(&(ch, pitch)) {
                                let start_s = ticks_to_secs(start);
                                let end_s = ticks_to_secs(abs_tick);
                                let dur = (end_s - start_s) as f32;
                                if dur > 0.0 {
                                    notes.push(Note {
                                        pitch: midi_to_pitch(pitch),
                                        start_time: start_s as f32,
                                        duration: dur,
                                        velocity: on_vel as f32 / 127.0,
                                    });
                                }
                            }
                        } else {
                            active.insert((ch, pitch), (velocity, abs_tick));
                        }
                    }
                    MidiMessage::NoteOff { key, .. } => {
                        let pitch = key.as_int();
                        if let Some((on_vel, start)) = active.remove(&(ch, pitch)) {
                            let start_s = ticks_to_secs(start);
                            let end_s = ticks_to_secs(abs_tick);
                            let dur = (end_s - start_s) as f32;
                            if dur > 0.0 {
                                notes.push(Note {
                                    pitch: midi_to_pitch(pitch),
                                    start_time: start_s as f32,
                                    duration: dur,
                                    velocity: on_vel as f32 / 127.0,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    notes.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
    Ok(Song { notes, bpm })
}

/// Helper: push a melody note (MIDI number, start in eighth-note units).
fn mel(notes: &mut Vec<Note>, midi: u8, start_8th: f32, dur_8ths: f32, eighth: f32) {
    notes.push(Note {
        pitch: midi_to_pitch(midi),
        start_time: start_8th * eighth,
        duration: dur_8ths * eighth * 0.9,
        velocity: 0.85,
    });
}

/// Helper: push a bass note.
fn bass(notes: &mut Vec<Note>, midi: u8, start_8th: f32, dur_8ths: f32, eighth: f32) {
    notes.push(Note {
        pitch: midi_to_pitch(midi),
        start_time: start_8th * eighth,
        duration: dur_8ths * eighth * 0.95,
        velocity: 0.55,
    });
}

/// Default demo: embedded MIDI file parsed at startup.
pub fn default_song() -> Song {
    let midi_data = include_bytes!("../coldplay-a_sky_full_of_stars.mid");
    parse_midi(midi_data).unwrap_or_else(|_| demo_song())
}

/// Axel F (Beverly Hills Cop theme) by Harold Faltermeyer — fallback.
pub fn demo_song() -> Song {
    let bpm = 116.0;
    let beat = 60.0 / bpm;
    let eighth = beat / 2.0;
    let mut notes = Vec::new();

    // MIDI note reference:
    // F4=65, Ab4=68, Bb4=70, Eb4=63, C5=72, Db5=73, F5=77
    // C4=60, G4=67, Db4=61, Ab3=56

    // ===== Phrase 1 (bars 1-2): the iconic riff =====
    // Bar 1: F4 . Ab4 . F4 F4 Bb4 F4
    mel(&mut notes, 65, 0.0, 1.0, eighth);
    mel(&mut notes, 68, 2.0, 1.0, eighth);
    mel(&mut notes, 65, 4.0, 0.5, eighth);
    mel(&mut notes, 65, 5.0, 1.0, eighth);
    mel(&mut notes, 70, 6.0, 1.0, eighth);
    mel(&mut notes, 65, 7.0, 1.0, eighth);

    // Bar 2: Eb4 . F4 . C5 . F4 F4
    mel(&mut notes, 63, 8.0, 1.0, eighth);
    mel(&mut notes, 65, 10.0, 1.0, eighth);
    mel(&mut notes, 72, 12.0, 1.0, eighth);
    mel(&mut notes, 65, 14.0, 0.5, eighth);
    mel(&mut notes, 65, 15.0, 1.0, eighth);

    // Bar 3: Db5 C5 Ab4 . F4 C5 F5 .
    mel(&mut notes, 73, 16.0, 1.0, eighth);
    mel(&mut notes, 72, 17.0, 1.0, eighth);
    mel(&mut notes, 68, 18.0, 1.0, eighth);
    mel(&mut notes, 65, 20.0, 1.0, eighth);
    mel(&mut notes, 72, 21.0, 1.0, eighth);
    mel(&mut notes, 77, 22.0, 1.0, eighth);

    // Bar 4: F4 Eb4 . C4 . G4 F4 .
    mel(&mut notes, 65, 24.0, 1.0, eighth);
    mel(&mut notes, 63, 25.0, 1.0, eighth);
    mel(&mut notes, 60, 27.0, 1.0, eighth);
    mel(&mut notes, 67, 29.0, 1.0, eighth);
    mel(&mut notes, 65, 30.0, 2.0, eighth);

    // ===== Phrase 2 (bars 5-8): repeat with variation =====
    let p2 = 32.0;
    // Bar 5: same as bar 1
    mel(&mut notes, 65, p2 + 0.0, 1.0, eighth);
    mel(&mut notes, 68, p2 + 2.0, 1.0, eighth);
    mel(&mut notes, 65, p2 + 4.0, 0.5, eighth);
    mel(&mut notes, 65, p2 + 5.0, 1.0, eighth);
    mel(&mut notes, 70, p2 + 6.0, 1.0, eighth);
    mel(&mut notes, 65, p2 + 7.0, 1.0, eighth);

    // Bar 6: same as bar 2
    mel(&mut notes, 63, p2 + 8.0, 1.0, eighth);
    mel(&mut notes, 65, p2 + 10.0, 1.0, eighth);
    mel(&mut notes, 72, p2 + 12.0, 1.0, eighth);
    mel(&mut notes, 65, p2 + 14.0, 0.5, eighth);
    mel(&mut notes, 65, p2 + 15.0, 1.0, eighth);

    // Bar 7: Db5 C5 Ab4 . F4 C5 F5 .
    mel(&mut notes, 73, p2 + 16.0, 1.0, eighth);
    mel(&mut notes, 72, p2 + 17.0, 1.0, eighth);
    mel(&mut notes, 68, p2 + 18.0, 1.0, eighth);
    mel(&mut notes, 65, p2 + 20.0, 1.0, eighth);
    mel(&mut notes, 72, p2 + 21.0, 1.0, eighth);
    mel(&mut notes, 77, p2 + 22.0, 1.0, eighth);

    // Bar 8: ending — Ab4 F4 . . . . . .
    mel(&mut notes, 68, p2 + 24.0, 1.0, eighth);
    mel(&mut notes, 65, p2 + 25.0, 3.0, eighth);

    // ===== Bass line (F minor) =====
    // Bars 1-4: F2 on beats 1 and 3
    for bar in 0..4u32 {
        let b = bar as f32 * 8.0;
        bass(&mut notes, 41, b, 2.0, eighth);       // F2 beat 1
        bass(&mut notes, 41, b + 4.0, 2.0, eighth); // F2 beat 3
    }
    // Bars 5-8: same pattern
    for bar in 4..8u32 {
        let b = bar as f32 * 8.0;
        let root = match bar {
            4 | 5 => 41, // F2
            6 => 44,     // Ab2
            _ => 41,     // F2
        };
        bass(&mut notes, root, b, 2.0, eighth);
        bass(&mut notes, root, b + 4.0, 2.0, eighth);
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
        assert!(song.notes.len() > 10);
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
