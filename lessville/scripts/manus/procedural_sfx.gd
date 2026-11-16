extends RefCounted
## Synthesize a short cue once, cache it, and play through the game's existing
## SFX manager. PCM WAV supports Web sample playback without a live generator.

static func tone(frequency_hz: float = 440.0, duration_seconds: float = 0.12, gain: float = 0.2, end_frequency_hz: float = -1.0, sample_rate: int = 22050) -> AudioStreamWAV:
	var rate: int = clampi(sample_rate, 8000, 48000)
	var duration: float = clampf(duration_seconds if is_finite(duration_seconds) else 0.12, 0.01, 2.0)
	var amplitude: float = clampf(gain if is_finite(gain) else 0.0, 0.0, 1.0)
	var start_hz: float = clampf(frequency_hz if is_finite(frequency_hz) else 440.0, 20.0, float(rate) * 0.45)
	var end_hz: float = clampf(end_frequency_hz, 20.0, float(rate) * 0.45) if is_finite(end_frequency_hz) and end_frequency_hz > 0.0 else start_hz
	var frames: int = maxi(2, roundi(duration * rate))
	var bytes := PackedByteArray()
	bytes.resize(frames * 2)
	var phase: float = 0.0
	var attack_frames: float = minf(rate * 0.005, frames * 0.25)
	var release_frames: float = minf(rate * 0.015, frames * 0.5)
	for index in range(frames):
		var progress: float = float(index) / float(frames - 1)
		var envelope: float = minf(1.0, minf(index / attack_frames, (frames - 1 - index) / release_frames))
		var sample: int = roundi(sin(phase) * amplitude * envelope * 32767.0)
		bytes.encode_s16(index * 2, sample)
		phase += TAU * lerpf(start_hz, end_hz, progress) / float(rate)
	var stream := AudioStreamWAV.new()
	stream.format = AudioStreamWAV.FORMAT_16_BITS
	stream.mix_rate = rate
	stream.stereo = false
	stream.loop_mode = AudioStreamWAV.LOOP_DISABLED
	stream.data = bytes
	return stream
