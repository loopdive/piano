#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Call the JS piano sampler: window._pianoPlay(midiNote, velocity, duration)
    #[wasm_bindgen(js_namespace = window, js_name = _pianoPlay, catch)]
    fn piano_play(midi_note: u32, velocity: f32, duration: f32) -> Result<(), JsValue>;

    /// Resume audio context on user gesture
    #[wasm_bindgen(js_namespace = window, js_name = _pianoResume, catch)]
    fn piano_resume() -> Result<(), JsValue>;
}

pub struct AudioPlayer;

impl AudioPlayer {
    pub fn new() -> Result<Self, JsValue> {
        Ok(Self)
    }

    /// Play a note using the JS piano sampler.
    /// pitch: 0-87 key index (added 21 to get MIDI note), velocity: 0.0-1.0, duration in seconds.
    pub fn play_note(&self, pitch: u8, velocity: f32, duration: f32) -> Result<(), JsValue> {
        let midi_note = pitch as u32 + 21; // pitch 0 = A0 = MIDI 21
        let _ = piano_resume();
        piano_play(midi_note, velocity, duration)
    }
}
