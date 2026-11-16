extends RefCounted

# The buffer source renders on Web Audio's audio thread. Godot only sends control
# changes; it never supplies PCM while a track is playing.
const SOURCE = """
(() => {
  if (globalThis.__manusBgm) return;
  // The existing TD campaign has 16 tracks totaling 344 MiB at 48 kHz.
  const MAX_BYTES = 512 * 1024 * 1024;
  const MAX_BUFFERS = 32;
  const buffers = new Map();
  const players = new Map();
  let context = null;
  let bytes = 0;
  let decoded = 0;
  let starts = 0;
  let listening = false;
  let pageHidden = false;
  let useClock = 0;
  const finite = (v, fallback = 0) => Number.isFinite(Number(v)) ? Number(v) : fallback;
  const now = () => context ? context.currentTime : 0;
  function resume() {
    if (!pageHidden && context && context.state !== 'closed' && context.state !== 'running') {
      const old = context;
      try { Promise.resolve(old.resume()).then(() => {
        if (context === old && pageHidden) suspendForCache(old);
      }).catch(() => {}); } catch (_) {}
    }
  }
  function listen() {
    if (listening) return;
    for (const event of ['pointerdown', 'touchend', 'keydown']) globalThis.addEventListener(event, resume, true);
    globalThis.addEventListener('pagehide', pageHide);
    globalThis.addEventListener('pageshow', pageShow);
    listening = true;
  }
  function unlisten() {
    if (!listening) return;
    for (const event of ['pointerdown', 'touchend', 'keydown']) globalThis.removeEventListener(event, resume, true);
    globalThis.removeEventListener('pagehide', pageHide);
    globalThis.removeEventListener('pageshow', pageShow);
    listening = false;
  }
  function ensureContext() {
    if (context) return context.state !== 'closed';
    try {
      const Constructor = globalThis.AudioContext || globalThis.webkitAudioContext;
      if (!Constructor) return false;
      context = new Constructor();
      listen();
      return true;
    } catch (_) { context = null; return false; }
  }
  function dropBuffer(key) {
    const entry = buffers.get(key);
    if (entry) { bytes -= entry.bytes; buffers.delete(key); }
  }
  function touch(entry) { entry.used = ++useClock; }
  function makeRoom(requiredBytes) {
    if (requiredBytes > MAX_BYTES) return false;
    const eligible = [...buffers.entries()].filter(([, entry]) => entry.ready && entry.references === 0)
      .sort((a, b) => a[1].used - b[1].used);
    let remainingBytes = bytes;
    let remainingCount = buffers.size;
    const victims = [];
    for (const [key, entry] of eligible) {
      if (remainingCount < MAX_BUFFERS && requiredBytes <= MAX_BYTES - remainingBytes) break;
      victims.push(key);
      remainingBytes -= entry.bytes;
      remainingCount -= 1;
    }
    if (remainingCount >= MAX_BUFFERS || requiredBytes > MAX_BYTES - remainingBytes) return false;
    for (const key of victims) dropBuffer(key);
    return true;
  }
  function teardown() {
    for (const player of [...players.values()]) player.dispose();
    unlisten();
    buffers.clear();
    bytes = 0;
    const old = context;
    context = null;
    pageHidden = false;
    try { if (old) Promise.resolve(old.close()).catch(() => {}); } catch (_) {}
  }
  function suspendForCache(old) {
    try {
      if (old && old.state !== 'closed') Promise.resolve(old.suspend()).then(() => {
        if (context === old && !pageHidden) resume();
      }).catch(() => {});
    } catch (_) {}
  }
  function pageHide(event) {
    if (!event.persisted) { teardown(); return; }
    // BFCache is a suspended page, not the end of its game. Freeze the audio
    // clock and preserve explicit player pause state and decoded resources.
    pageHidden = true;
    suspendForCache(context);
  }
  function pageShow(event) {
    if (event.persisted) { pageHidden = false; resume(); }
  }
  function channel(id) {
    if (!ensureContext()) return null;
    if (players.has(id)) return players.get(id);
    let gain;
    try { gain = context.createGain(); gain.connect(context.destination); }
    catch (_) {
      try { gain?.disconnect(); } catch (_) {}
      if (!players.size && !buffers.size) teardown();
      return null;
    }
    let source = null;
    let entry = null;
    let duration = 0;
    let active = false;
    let paused = false;
    let disposed = false;
    let failed = false;
    let offset = 0;
    let anchor = 0;
    let pitch = 1;
    let level = 1;
    let looping = false;
    let loopStart = 0;
    let loopEnd = 0;
    function releaseEntry() {
      if (!entry) return;
      entry.references -= 1;
      touch(entry);
      entry = null;
    }
    function normalize(value) {
      if (!duration) return 0;
      value = Math.max(0, finite(value));
      if (looping && value >= loopEnd) return loopStart + (value - loopStart) % (loopEnd - loopStart);
      return Math.min(value, duration);
    }
    function position() {
      return normalize(offset + (active && !paused && source ? Math.max(0, now() - anchor) * pitch : 0));
    }
    function disconnect() {
      if (!source) return;
      const old = source;
      source = null;
      old.onended = null;
      try { old.stop(); } catch (_) {}
      try { old.disconnect(); } catch (_) {}
    }
    function start() {
      if (disposed || failed || !active || paused || !entry || context.state === 'closed') return false;
      try {
        const node = context.createBufferSource();
        node.buffer = entry.buffer;
        node.loop = looping;
        node.loopStart = loopStart;
        node.loopEnd = loopEnd;
        node.playbackRate.value = pitch;
        node.connect(gain);
        node.onended = () => {
          if (source !== node) return;
          offset = duration;
          active = false;
          source = null;
          node.disconnect();
          releaseEntry();
        };
        source = node;
        anchor = now();
        node.start(0, normalize(offset));
        starts += 1;
        // A suspended context is an autoplay lock, not a backend failure.
        resume();
        return true;
      } catch (_) { disconnect(); active = false; failed = true; releaseEntry(); return false; }
    }
    const player = {
      play(bufferKey, from = 0, loop = false, begin = 0, end = 0) {
        if (disposed || failed || context.state === 'closed') return false;
        const next = buffers.get(String(bufferKey));
        if (!next || !next.ready) return false;
        disconnect();
        releaseEntry();
        entry = next;
        entry.references += 1;
        touch(entry);
        duration = entry.buffer.duration;
        looping = Boolean(loop);
        loopStart = Math.min(Math.max(0, finite(begin)), Math.max(0, entry.buffer.duration - 1 / entry.buffer.sampleRate));
        loopEnd = Math.min(entry.buffer.duration, finite(end) > loopStart ? finite(end) : entry.buffer.duration);
        offset = normalize(from);
        active = true;
        return paused || start();
      },
      stop() { disconnect(); active = false; offset = 0; releaseEntry(); },
      setPaused(value) {
        value = Boolean(value);
        if (disposed || paused === value) return;
        offset = position();
        paused = value;
        if (paused) disconnect();
        else if (active) start();
      },
      setGain(value) {
        level = Math.max(0, finite(value));
        if (!disposed) gain.gain.setValueAtTime(level, now());
      },
      setPitch(value) {
        value = Math.max(0.01, finite(value, 1));
        if (disposed || pitch === value) return;
        offset = position();
        anchor = now();
        pitch = value;
        if (source) source.playbackRate.setValueAtTime(pitch, now());
      },
      seek(value) {
        if (disposed) return;
        offset = normalize(value);
        disconnect();
        if (active && !paused) start();
      },
      position,
      playing() { return active; },
      healthy() { return !disposed && !failed && context !== null && context.state !== 'closed'; },
      dispose() {
        if (disposed) return;
        player.stop();
        disposed = true;
        try { gain.disconnect(); } catch (_) {}
        players.delete(id);
      },
    };
    players.set(id, player);
    return player;
  }
  globalThis.__manusBgm = {
    createPlayer: channel,
    teardown,
    hasBuffer(key) {
      const entry = buffers.get(String(key));
      if (entry?.ready) { touch(entry); return true; }
      return false;
    },
    beginBuffer(key, frames, sampleRate) {
      key = String(key);
      frames = Number(frames);
      sampleRate = Number(sampleRate);
      if (!ensureContext() || buffers.has(key) || !Number.isSafeInteger(frames) || frames <= 0 ||
          !Number.isFinite(sampleRate) || sampleRate < 8000 || sampleRate > 192000 ||
          !makeRoom(frames * 8)) return false;
      try {
        const buffer = context.createBuffer(2, frames, sampleRate);
        buffers.set(key, { buffer, ready: false, written: 0, bytes: frames * 8, references: 0, used: ++useClock });
        bytes += frames * 8;
        return true;
      } catch (_) { return false; }
    },
    appendBuffer(key, encoded, frameOffset) {
      key = String(key);
      const entry = buffers.get(key);
      if (!entry || entry.ready || Number(frameOffset) !== entry.written) return false;
      try {
        const raw = atob(String(encoded));
        if (raw.length % 8 || !raw.length || entry.written + raw.length / 8 > entry.buffer.length) return false;
        const packed = new Uint8Array(raw.length);
        for (let i = 0; i < raw.length; i++) packed[i] = raw.charCodeAt(i);
        const view = new DataView(packed.buffer);
        const left = entry.buffer.getChannelData(0);
        const right = entry.buffer.getChannelData(1);
        for (let i = 0; i < raw.length / 8; i++) {
          const l = view.getFloat32(i * 8, true);
          const r = view.getFloat32(i * 8 + 4, true);
          if (!Number.isFinite(l) || !Number.isFinite(r)) return false;
          left[entry.written + i] = l;
          right[entry.written + i] = r;
        }
        entry.written += raw.length / 8;
        return true;
      } catch (_) { return false; }
    },
    finishBuffer(key, written) {
      const entry = buffers.get(String(key));
      if (!entry || entry.ready) return false;
      const frames = written === undefined ? entry.buffer.length : Number(written);
      if (!Number.isSafeInteger(frames) || frames <= 0 || frames !== entry.written || frames > entry.buffer.length) return false;
      if (frames < entry.buffer.length) {
        try {
          const trimmed = context.createBuffer(2, frames, entry.buffer.sampleRate);
          for (let channel = 0; channel < 2; channel++) trimmed.getChannelData(channel).set(entry.buffer.getChannelData(channel).subarray(0, frames));
          bytes -= entry.bytes - frames * 8;
          entry.bytes = frames * 8;
          entry.buffer = trimmed;
        } catch (_) { return false; }
      }
      entry.ready = true;
      touch(entry);
      decoded += 1;
      return true;
    },
    cancelBuffer(key) {
      key = String(key);
      if (!buffers.get(key)?.ready) dropBuffer(key);
    },
    releaseBuffer(key) {
      key = String(key);
      const entry = buffers.get(key);
      if (!entry) return true;
      if (!entry.ready || entry.references !== 0) return false;
      dropBuffer(key);
      return true;
    },
    diagnostics() {
      return JSON.stringify({ buffers: buffers.size, bytes, decoded, starts,
        players: players.size, playing: [...players.values()].filter(p => p.playing()).length,
        state: context?.state || 'uninitialized' });
    },
  };
})();
"""
