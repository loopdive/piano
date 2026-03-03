# 002: Manual Key Input (Mouse, Touch, Computer Keyboard)

## Summary

Allow the user to press piano keys manually via mouse click, touch tap, or computer keyboard keys. Support multiple simultaneous key presses (multitouch and multi-key).

## Requirements

### Mouse/Touch Input
- Clicking or tapping on a rendered piano key plays the corresponding note
- Dragging across keys plays each key as the pointer enters it (glissando)
- Multiple simultaneous touch points each trigger independent notes (multitouch)
- Keys visually depress while held (using existing key_press_state animation)

### Computer Keyboard Input
- Map a range of computer keys to piano keys (e.g., A–L for white keys, W–P for black keys in one octave)
- Multiple keys held simultaneously all sound at once
- Octave shift with Z/X or similar to move the mapped range up/down

### Audio
- Use the existing AudioPlayer/piano sampler to play notes on press
- Note-off (release) should stop/fade the note

## Affected Files

- `src/lib.rs` — input handling for mouse clicks in keyboard area, keyboard key mapping, touch handling
- `src/keyboard.rs` — hit-testing: given (x, y) screen position, return which pitch was clicked
- `src/audio.rs` — may need note-off support (currently fire-and-forget)

## Notes

- Black keys overlay white keys — hit testing must check black keys first
- Computer keyboard mapping: common "piano keyboard" layout (QWERTY → piano keys)
- Need to track which keys are manually held vs. auto-played by the MIDI playback
