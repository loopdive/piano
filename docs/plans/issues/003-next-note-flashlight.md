# 003: Next-Note Flashlight on Keys

## Summary

Show a visual indicator (flashlight/highlight) on the piano keys that need to be pressed next, helping the user follow along or learn the piece.

## Requirements

- Highlight keys that correspond to the next upcoming note(s) in the MIDI timeline
- The highlight should appear slightly before the note reaches the keyboard (look-ahead window)
- Multiple simultaneous upcoming notes should all be highlighted
- Visual style: a glow or bright overlay on the key surface, distinct from the "currently playing" press animation
- The highlight should fade in as the note approaches and be brightest just before the note arrives

## Affected Files

- `src/lib.rs` — compute upcoming notes within look-ahead window, pass highlight data to keyboard rendering
- Keyboard 3D key rendering section — add highlight color/glow to keys in the "next" set

## Design Considerations

- Look-ahead window: ~0.5–1.0 seconds before the note reaches the keyboard
- Color: could use a warm color (orange/yellow) to contrast with the blue notes, or a pulsing white glow
- Should be toggleable (some users may find it distracting)
- When paused, highlight the notes that will play when resumed
- During manual play mode (issue 002), could show which keys to press to match the score
