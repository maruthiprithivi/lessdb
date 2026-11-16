# Optional audio contract

The template ships with **zero default BGM, SFX, or UI audio assets**, as
explicitly requested for the minimalist redesign. Silence is intentional.
There are no placeholder generators, automatic tones, or network audio fetches.

`autoload/audio_director.gd` retains Master/Music/SFX/UI buses, two streaming
music voices, eight SFX voices, two UI voices, crossfade/looping, gesture unlock,
button subscription deduplication, pause ducking, mute and per-bus volume.

Assign an imported project `AudioStream` with
`AudioDirector.register_cue("score.reward", "res://assets/template/audio/reward.ogg")`
or register a stream directly with `register_stream()`. Populate the optional
paths in `CUES` for startup registration. Use a single track for the logical
`music.title` and `music.gameplay` routes when adding music to a new game.

Empty/missing paths, null streams, wrong resource types and unknown cue IDs
return `false`; they never synthesize replacement audio. A missing music route
stops any previous track. An invalid replacement unregisters the prior cue.
Missing cue playback creates no nodes. The deterministic test suite verifies
these states with a tiny in-memory audio fixture that never ships as an asset.

The shared new-game audio policy continues to govern future adaptations;
this template's silent baseline records the owner's explicit exception.
