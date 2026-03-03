#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext, Response};

pub struct AudioPlayer {
    ctx: AudioContext,
    samples: Vec<Option<AudioBuffer>>,
}

impl AudioPlayer {
    pub fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        Ok(Self {
            ctx,
            samples: vec![None; 7],
        })
    }

    pub async fn resume(&self) -> Result<(), JsValue> {
        JsFuture::from(self.ctx.resume()?).await?;
        Ok(())
    }

    pub async fn load_sample(&mut self, octave_index: usize, url: &str) -> Result<(), JsValue> {
        let window = web_sys::window().unwrap();
        let mut opts = web_sys::RequestInit::new();
        opts.method("GET");
        opts.mode(web_sys::RequestMode::SameOrigin);
        let request = web_sys::Request::new_with_str_and_init(url, &opts)?;
        let resp: Response = JsFuture::from(window.fetch_with_request(&request))
            .await?
            .dyn_into()?;
        let array_buffer: js_sys::ArrayBuffer =
            JsFuture::from(resp.array_buffer()?).await?.dyn_into()?;
        let audio_buffer: AudioBuffer =
            JsFuture::from(self.ctx.decode_audio_data(&array_buffer)?)
                .await?
                .dyn_into()?;
        if octave_index < self.samples.len() {
            self.samples[octave_index] = Some(audio_buffer);
        }
        Ok(())
    }

    pub fn play_note(&self, pitch: u8, _velocity: f32) -> Result<(), JsValue> {
        // Sample pitches: C1(3), C2(15), C3(27), C4(39), C5(51), C6(63), C7(75)
        let sample_pitches: [u8; 7] = [3, 15, 27, 39, 51, 63, 75];

        let mut best_idx = 0;
        let mut best_dist = 128i16;
        for (i, &sp) in sample_pitches.iter().enumerate() {
            let dist = (pitch as i16 - sp as i16).abs();
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }

        if let Some(buffer) = &self.samples[best_idx] {
            let source: AudioBufferSourceNode = self.ctx.create_buffer_source()?;
            source.set_buffer(Some(buffer));
            let semitone_diff = pitch as f32 - sample_pitches[best_idx] as f32;
            let rate = 2.0_f32.powf(semitone_diff / 12.0);
            source.playback_rate().set_value(rate);
            source.connect_with_audio_node(&self.ctx.destination())?;
            source.start()?;
        }

        Ok(())
    }
}
