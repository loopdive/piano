# 001: Horizontal Keyboard Scrolling & Auto-Center

## Summary

Dragging the keyboard left or right should shift it and the note bands horizontally. By default, the keyboard should be centered on the mid-range of notes in the loaded MIDI file.

## Requirements

- Horizontal drag gesture on the keyboard area shifts the entire view (keyboard + note lanes) left/right
- The horizontal offset applies to both the keyboard rendering and the note fall area
- On MIDI load, auto-center the view so the middle of the note pitch range is centered on screen
- Scrolling should be smooth with inertia (matching existing vertical scroll behavior)
- Clamp horizontal offset so the keyboard doesn't scroll entirely off-screen

## Affected Files

- `src/lib.rs` — add `h_offset` state, horizontal drag detection, auto-center on MIDI load
- `src/keyboard.rs` — `key_rect()` needs to accept an offset parameter or the offset is applied at render time
- Note rendering, grid lines, glow — all need horizontal offset applied

## Notes

- Need to distinguish horizontal vs vertical drag (dominant axis detection)
- Touch drag already exists for vertical scrubbing — horizontal drag should coexist
- The keyboard is currently 49 white keys (C1–B7); many MIDI files only use a portion
