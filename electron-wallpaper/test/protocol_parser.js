// Standalone reimplementation of electron-wallpaper/main.js's stdin framing loop.
// MIRRORS main.js — keep in sync if the protocol changes there.
// Extracted for testability without launching real Electron (which needs a display).

const TAG_AUDIO = 0;
const AUDIO_BYTES = 1 + (128 + 1) * 4; // tag + 128 f32 bands + 1 f32 energy

// Feed bytes via push(); call reset() between scenarios. Captures emitted frames.
class FramingParser {
  constructor() {
    this.buf = Buffer.alloc(0);
    this.events = []; // [{ type: 'audio'|'config'|'lyrics'|'playback'|'theme', data }]
    this.winAlive = true;
  }
  reset() { this.buf = Buffer.alloc(0); this.events = []; }
  push(chunk) {
    this.buf = Buffer.concat([this.buf, chunk]);
    while (this.buf.length > 0) {
      const tag = this.buf[0];
      if (tag === TAG_AUDIO) {
        if (this.buf.length < AUDIO_BYTES) break;
        const bands = new Array(128);
        for (let i = 0; i < 128; i++) bands[i] = this.buf.readFloatLE(1 + i * 4);
        const energy = this.buf.readFloatLE(1 + 128 * 4);
        this.events.push({ type: 'audio', data: { bands, energy } });
        this.buf = this.buf.slice(AUDIO_BYTES);
        continue;
      }
      if (tag >= 1 && tag <= 4) {
        if (this.buf.length < 5) break;
        const len = this.buf.readUInt32LE(1);
        if (len === 0 || len > 1024 * 1024) {
          this.buf = this.buf.slice(1); // drop corrupt
          continue;
        }
        if (this.buf.length < 5 + len) break;
        let payload = null;
        try { payload = JSON.parse(this.buf.slice(5, 5 + len).toString('utf8')); } catch (_) {}
        this.buf = this.buf.slice(5 + len);
        if (payload == null || !this.winAlive) continue;
        const type = { 1: 'config', 2: 'lyrics', 3: 'playback', 4: 'theme' }[tag];
        if (type) this.events.push({ type, data: payload });
        continue;
      }
      // Unknown byte: discard only that byte (resync).
      this.buf = this.buf.slice(1);
    }
  }
}

module.exports = { FramingParser, AUDIO_BYTES, TAG_AUDIO };
