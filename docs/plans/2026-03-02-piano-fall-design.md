# Piano Fall — Design Document

## Overview

A web application built with Rust, wgpu, and WebAssembly that renders a piano waterfall visualization: glowing notes scroll from top to bottom and "play" on a piano keyboard at the bottom of the screen, triggering piano sample sounds via the WebAudio API.

## Goals

- Falling note blocks mapped to an 88-key piano layout
- Full bloom/glow post-processing effect on notes
- Particle trails at note-keyboard contact points
- Piano sample audio triggered on note hit
- Runs in the browser via wasm32, native build for visual testing

## Non-Goals (for now)

- MIDI file import (future enhancement)
- User keyboard input / interactive play mode
- Recording or score tracking

## Architecture

### Project Location

`wgpu/piano-fall/` — standalone Cargo project.

### Data Model

```rust
struct Note {
    pitch: u8,        // 0-87 (A0=0, C8=87)
    start_time: f32,  // seconds from song start
    duration: f32,    // seconds
    velocity: f32,    // 0.0-1.0
}

struct Song {
    notes: Vec<Note>,
    bpm: f32,
}

struct Particle {
    pos: [f32; 2],
    vel: [f32; 2],
    alpha: f32,
    color: [f32; 3],
    life: f32,
}
```

### Keyboard Layout

- 88 keys: 52 white, 36 black
- Standard piano mapping: A0 (pitch 0) through C8 (pitch 87)
- White keys pattern per octave: C D E F G A B
- Black keys: C# D# F# G# A#
- Note X-position = corresponding key X-position
- Note width = key width (white or black depending on pitch)

### Screen Layout

- Top ~80%: Note fall area (black background)
- Bottom ~20%: Piano keyboard
- Notes scroll downward at constant speed

## Rendering Pipeline

5-pass multi-pass rendering:

### Pass 1: Scene (offscreen texture)

- Clear to black
- Draw falling notes: instanced quads, bright blue/cyan color
- Draw keyboard: static quads, white/gray keys, active keys glow
- Draw particles: instanced small quads with alpha blending

### Pass 2: Bright Extract (offscreen texture)

- Full-screen quad
- Fragment shader: extract pixels above brightness threshold
- Output: only the bright/glowing parts

### Pass 3: Horizontal Gaussian Blur

- Full-screen quad
- 9-13 tap Gaussian blur in X direction

### Pass 4: Vertical Gaussian Blur

- Full-screen quad
- 9-13 tap Gaussian blur in Y direction

### Pass 5: Composite (screen)

- Full-screen quad
- Blend: scene texture + bloom texture (additive)
- Output to swapchain

### Shaders

| File | Purpose |
|------|---------|
| `note.wgsl` | Instanced note rectangles with color/glow |
| `keyboard.wgsl` | Keyboard keys with hit-glow animation |
| `particle.wgsl` | Particles with alpha fade |
| `bloom_extract.wgsl` | Threshold bright pixels |
| `blur.wgsl` | Configurable H/V Gaussian blur |
| `composite.wgsl` | Blend scene + bloom |

### Note Visuals

- Color gradient by pitch: deep blue (low) → cyan (mid) → white-blue (high)
- Glow intensity proportional to velocity
- When a note hits the keyboard line: brighter glow, particle emission

### Particle System

- Emitter at each active note's keyboard contact point
- ~20 particles/second per active note
- Particles drift upward slightly, fade over 0.5-1s
- Color matches parent note color
- Rendered as instanced alpha-blended quads

## Audio

### Architecture (wasm32 only)

- Access WebAudio API via `web-sys` (`AudioContext`, `AudioBuffer`, `AudioBufferSourceNode`)
- On startup: fetch and decode piano samples
- On note hit: create `AudioBufferSourceNode`, set `playbackRate` for pitch, connect to output, play

### Sample Strategy

- 8 piano samples (1 per octave: C1, C2, C3, C4, C5, C6, C7, C8)
- In-between notes pitch-shifted via `playbackRate` (max ±6 semitones)
- Samples as `.mp3` files in `web/assets/samples/`
- Free/open-source piano samples

### Native Build

- No audio on native — visual testing only

## Dependencies

| Crate | Purpose |
|-------|---------|
| `wgpu` | GPU rendering |
| `winit` | Window/event management |
| `wasm-bindgen` | JS interop (wasm32) |
| `web-sys` | WebAudio + DOM access (wasm32) |
| `bytemuck` | Safe vertex buffer casting |
| `glam` | Vector/matrix math |
| `console_error_panic_hook` | Panic debugging (wasm32) |
| `console_log` | Logging (wasm32) |

## File Structure

```
wgpu/piano-fall/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Entry point, event loop
│   ├── main.rs             # Native entry (pollster)
│   ├── state.rs            # wgpu State + pipeline setup
│   ├── note.rs             # Note model, demo song
│   ├── keyboard.rs         # 88-key layout calculations
│   ├── renderer/
│   │   ├── mod.rs
│   │   ├── notes.rs        # Instanced note rendering
│   │   ├── keyboard.rs     # Keyboard quad rendering
│   │   ├── particles.rs    # Particle system
│   │   └── bloom.rs        # Bloom post-processing
│   ├── audio.rs            # WebAudio sample playback (wasm32)
│   └── shaders/
│       ├── note.wgsl
│       ├── keyboard.wgsl
│       ├── particle.wgsl
│       ├── bloom_extract.wgsl
│       ├── blur.wgsl
│       └── composite.wgsl
├── web/
│   ├── index.html
│   └── assets/
│       └── samples/        # Piano .mp3 samples
└── build.sh                # wasm-pack build script
```

## Demo Song

Hardcoded note sequence — a simple melody (C major scale ascending/descending + basic chords) to demonstrate the visualization. Designed to show notes of varying pitch, duration, and simultaneous playback.
