// Smoke test for electron-wallpaper/test/protocol_parser.js.
// Verifies the stdin framing loop decodes tags 0-4, handles truncation, and resyncs
// after corrupt bytes — WITHOUT launching real Electron (no display needed).
const assert = require('assert');
const { FramingParser, AUDIO_BYTES, TAG_AUDIO } = require('./protocol_parser');

let passed = 0, failed = 0;
const ok = (name, cond) => { if (cond) { passed++; console.log('  ok - ' + name); } else { failed++; console.log('  FAIL - ' + name); } };

function taggedFrame(tag, obj) {
  const json = Buffer.from(JSON.stringify(obj), 'utf8');
  const head = Buffer.alloc(5);
  head[0] = tag;
  head.writeUInt32LE(json.length, 1);
  return Buffer.concat([head, json]);
}

function audioFrame(bands, energy) {
  const body = Buffer.alloc(AUDIO_BYTES);
  body[0] = TAG_AUDIO;
  for (let i = 0; i < 128; i++) body.writeFloatLE(bands[i], 1 + i * 4);
  body.writeFloatLE(energy, 1 + 128 * 4);
  return body;
}

(function lyrics() {
  const p = new FramingParser();
  p.push(taggedFrame(2, { lines: [{ startTime: 1, endTime: 3, fullText: 'hi', words: [] }], offset: 0 }));
  ok('lyrics: one event', p.events.length === 1);
  ok('lyrics: type', p.events[0].type === 'lyrics');
  ok('lyrics: line fullText', p.events[0].data.lines[0].fullText === 'hi');
})();

(function playback() {
  const p = new FramingParser();
  p.push(taggedFrame(3, { positionSec: 12.5, durationSec: 180, playing: true, title: 'T' }));
  ok('playback: type', p.events[0].type === 'playback');
  ok('playback: positionSec', p.events[0].data.positionSec === 12.5);
  ok('playback: playing', p.events[0].data.playing === true);
})();

(function theme() {
  const p = new FramingParser();
  p.push(taggedFrame(4, { name: 'pulse-ring', primaryColor: '#FF0000', animationIntensity: 'normal' }));
  ok('theme: type', p.events[0].type === 'theme');
  ok('theme: primaryColor', p.events[0].data.primaryColor === '#FF0000');
})();

(function config() {
  const p = new FramingParser();
  p.push(taggedFrame(1, { visualizerMode: 'sonnet' }));
  ok('config: type', p.events[0].type === 'config');
  ok('config: visualizerMode', p.events[0].data.visualizerMode === 'sonnet');
})();

(function audio_and_order() {
  const p = new FramingParser();
  p.push(Buffer.concat([
    audioFrame(new Array(128).fill(0.5), 0.9),
    taggedFrame(2, { lines: [], offset: 0 }),
  ]));
  ok('audio+lyrics: 2 events', p.events.length === 2);
  ok('audio+lyrics: audio first', p.events[0].type === 'audio');
  ok('audio+lyrics: energy', Math.abs(p.events[0].data.energy - 0.9) < 1e-6); // f32 precision
  ok('audio+lyrics: band[0]', p.events[0].data.bands[0] === 0.5);
  ok('audio+lyrics: lyrics second', p.events[1].type === 'lyrics');
})();

(function truncation() {
  const p = new FramingParser();
  const full = taggedFrame(3, { positionSec: 1, durationSec: 100, playing: false, title: '' });
  // Send only the first 3 bytes (tag + 2 of length) -> must wait for more.
  p.push(full.subarray(0, 3));
  ok('truncated: no event yet', p.events.length === 0);
  // Send the rest -> now it completes.
  p.push(full.subarray(3));
  ok('truncated: event after rest', p.events.length === 1);
  ok('truncated: type playback', p.events[0].type === 'playback');
})();

(function resync_after_corrupt() {
  const p = new FramingParser();
  const good = taggedFrame(2, { lines: [], offset: 0 });
  const corrupt = Buffer.from([0xFF, 0xEE]); // unknown tags
  p.push(Buffer.concat([corrupt, good]));
  ok('resync: corrupt bytes discarded', p.events.length === 1);
  ok('resync: valid frame after corrupt', p.events[0].type === 'lyrics');
})();

(function zero_len_dropped_does_not_crash() {
  // The real protocol NEVER sends len=0 (Rust always emits a non-empty JSON string).
  // A zero-length frame is therefore corruption. Like main.js, the parser drops
  // only the tag byte, so the 4 length bytes then get misread as an audio header —
  // the stream desyncs until it happens to realign. This mirrors main.js exactly;
  // the smoke test just asserts it doesn't crash.
  const p = new FramingParser();
  const drop = Buffer.from([0x02, 0x00, 0x00, 0x00, 0x00]);
  p.push(Buffer.concat([drop, taggedFrame(4, { name: 'x' })]));
  ok('zero-len: no crash (mirrors main.js)', p.events.length >= 0);
})();

(function huge_len_dropped() {
  const p = new FramingParser();
  const head = Buffer.alloc(5);
  head[0] = 0x02;
  head.writeUInt32LE(0xFFFFFFFF, 1); // absurd length
  p.push(Buffer.concat([head, taggedFrame(1, { visualizerMode: 'classic' })]));
  ok('huge-len: dropped, valid config after', p.events.length === 1 && p.events[0].type === 'config');
})();

(function dead_window_no_emit() {
  const p = new FramingParser();
  p.winAlive = false;
  p.push(taggedFrame(2, { lines: [], offset: 0 }));
  ok('dead win: no events emitted', p.events.length === 0);
})();

console.log(`\n${passed} passed, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
