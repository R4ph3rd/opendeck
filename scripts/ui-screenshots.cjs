// UI screenshot harness: loads the built UI (vite preview), mocks Tauri IPC in the page,
// loads two tracks, writes PNGs and prints a clipping report per viewport and pad mode.
// Usage: see docs/BUILD.md ("UI screenshots").
const path = require("node:path");

const loadPlaywright = () => {
  try {
    return require("playwright");
  } catch {
    if (process.env.PLAYWRIGHT_PATH) return require(process.env.PLAYWRIGHT_PATH);
    throw new Error("playwright not found: install it globally or set PLAYWRIGHT_PATH");
  }
};
const { chromium } = loadPlaywright();

const BASE = process.env.BASE || "http://127.0.0.1:4173/";
const OUT = process.env.OUT || path.join(__dirname, "..", "target", "ui-screenshots");
const SIZES = (process.env.SIZES || "1440x900,1920x1080").split(",").map((s) => s.split("x").map(Number));
const TAG = process.env.TAG || "ui";
const MODES = (process.env.MODES || "hot_cue,pad_fx1").split(",");

const init = () => {
  const cbs = new Map();
  const listeners = new Map();
  let next = 1;
  const ok = (v) => v;
  const key = (id) => ({ provider: "builtin", id });
  const eff = (id, name, beat) => ({
    key: key(id), name, routing: "insert", beat_synced: beat, has_tail: false,
    params: [{ id: 0, name: "Amount", unit: "percent", min: 0, max: 100, default: 50, curve: { kind: "linear" } }],
    primary: 0, release: false, divisions: beat ? [0.25, 0.5, 1, 2] : [],
  });
  const effects = [eff("echo", "Echo", true), eff("reverb", "Reverb", false), eff("flanger", "Flanger", true),
    eff("vinyl_brake", "Vinyl Brake", true), eff("colour_filter", "Colour Filter", false), eff("noise", "Noise", false),
    eff("roll", "Roll", true), eff("phaser", "Phaser", true)];
  const slot = (e) => ({ effect: e ? key(e) : null, params: [0.5], wet: 0.5, division: 1, enabled: true });
  const fxState = {
    units: [0, 1].map((i) => ({ assign: i ? "ch2" : "ch1", mode: "multi", slots: [slot("echo"), slot(null), slot(null)], on: false, bpm_source: "auto", tap_bpm: null, release_effect: key("vinyl_brake"), release_hold_ms: 2000 })),
    cfx: [0, 1].map(() => ({ kind: key("colour_filter"), amount: 0 })),
    cfx_kinds: [key("colour_filter"), key("noise")],
    sends: [0, 1].map(() => ({ slots: [slot("reverb"), slot(null), slot(null)], post_fader: true })),
    send_levels: [[0, 0], [0, 0]],
    pads: Array.from({ length: 32 }, (_, i) => ({ effect: key(["echo", "reverb", "flanger", "vinyl_brake", "roll", "phaser", "noise", "colour_filter"][i % 8]), division: 0.5, release: i % 8 === 3 })),
  };
  const mixer = {
    channels: [0, 1, 2, 3].map((deck) => ({ deck, trim: 1, eq: [1, 1, 1], filter: 0, fader: 0.85, pfl: deck === 1, crossfader_assign: "thru" })),
    crossfader: 0, crossfader_curve: "constant_power", master_level: 1, headphone_level: 1, headphone_mix: 0, split_cue: false,
  };
  const padState = { decks: [0, 1].map((deck) => ({ deck, mode: "hot_cue", jump_beats: [-1, 1, -2, 2, -4, 4, -8, 8], loop_beats: [0.25, 0.5, 1, 2, 4, 8, 16, 32], key_semitones: [4, 5, 6, 7, 0, 1, 2, 3], keyboard_cue: null, keyboard_semitones: [4, 5, 6, 7, 0, 1, 2, 3] })) };
  const names = ["Kick loop", "Vox hit", "Riser", "Impact", "Clap", "", "", ""];
  const sampler = { layout: "split", level: 1, pfl: false, slots: Array.from({ length: 16 }, (_, s) => ({ slot: s, name: names[s % 8] ?? "", path: names[s % 8] ? "/s/x.wav" : null, track: null, color: s % 8, gain: 1, play_mode: "one_shot", sync: false, loaded: !!names[s % 8], loading: false, duration_secs: 1.5 + (s % 5), peaks: [] })) };
  const rows = Array.from({ length: 40 }, (_, i) => ({
    id: i + 1, path: `/m/${i}.mp3`, title: ["LSD Party (Meltdown)", "Night Drive", "Derbouka", "Flamingo", "Eternal"][i % 5] + (i > 4 ? ` ${i}` : ""),
    artist: ["Mad Tribe - Topic", "Test Artist", "Infected Mushroom", "ETERNAL - Topic"][i % 4], album: "Album " + (i % 6), genre: "Psytrance", year: 2020, comment: null,
    duration_secs: 300 + i, bitrate_kbps: 320, bpm: 140 + (i % 9), key: `${(i % 12) + 1}A`, key_musical: "Am", rating: i % 6, color: null, date_added: 1.7e12 + i, analysis_status: "done", missing: false, artwork_id: null,
  }));
  const peaks = (level) => {
    const fpp = [512, 2048, 8192, 32768, 131072][level] || 8192;
    const count = Math.floor((330 * 48000) / fpp);
    const buf = new ArrayBuffer(24 + count * 4);
    const dv = new DataView(buf);
    "ODPK".split("").forEach((c, i) => dv.setUint8(i, c.charCodeAt(0)));
    dv.setUint16(4, 1, true); dv.setUint16(6, 4, true); dv.setUint32(8, 48000, true); dv.setUint32(12, fpp, true); dv.setUint32(16, count, true);
    const u8 = new Uint8Array(buf);
    for (let i = 0; i < count; i++) {
      const t = i / count;
      const env = 0.35 + 0.55 * Math.abs(Math.sin(t * 38)) * (t > 0.08 && t < 0.92 ? 1 : 0.3);
      const jitter = 0.85 + 0.15 * Math.sin(i * 0.9);
      const p = Math.min(255, Math.round(255 * env * jitter));
      u8[24 + i * 4] = p; u8[25 + i * 4] = Math.round(p * 0.95); u8[26 + i * 4] = Math.round(p * 0.6); u8[27 + i * 4] = Math.round(p * 0.35);
    }
    return Array.from(u8.slice(0, 24 + count * 4));
  };
  const handlers = {
    get_audio_status: () => ({ running: true, device: "Built-in Output", sample_rate: 48000 }),
    get_mixer_state: () => mixer,
    get_deck_state: ({ deck }) => ({ deck, track: null, play_state: "empty", cue_point: deck === 0 ? 12.5 : null, track_bpm: null, track_key: null, beat_grid: null, tempo_range: "percent6", tempo_fader: deck === 0 ? 0 : -0.33, key_lock: deck === 0, key_shift: 0, sync: deck === 1 ? "beat" : "off", is_leader: deck === 0, slip: false, quantize: true, vinyl_mode: true, active_loop: null, pad_mode: "hot_cue" }),
    fx_get_state: () => fxState, fx_list_effects: () => effects, fx_get_presets: () => ({ factory: [], user: [], chains: [] }),
    get_pad_state: () => padState, get_sampler_state: () => sampler,
    get_ui_pref: ({ name }) => (name === "firstRunDone" ? "true" : null),
    get_startup_notices: () => [],
    midi_get_status: () => ({ mapping_id: "pioneer-ddj-flx4", mapping_name: "Pioneer DDJ-FLX4", enabled: true, connected: true, input: "DDJ-FLX4", output: null, last_error: null, auto_connect: true, device: null, fader_start: false, dropped_input: 0, verify_count: 165 }),
    query_tracks: () => ({ rows, total: rows.length }),
    list_playlists: () => [], list_saved_sets: () => [], list_jobs: () => [], midi_list_ports: () => [],
    get_track_peaks: ({ level }) => peaks(level ?? 0), get_artwork: () => [], list_cues: () => [], get_analysis: () => null,
    list_directory: () => ({ path: "/music", parent: "/", entries: [] }),
    get_bpm_range: () => ({ min_bpm: 70, max_bpm: 180 }), midi_get_mapping: () => [],
    supported_audio_extensions: () => ["wav", "mp3"],
    "plugin:event|listen": ({ event, handler }) => { (listeners.get(event) || listeners.set(event, []).get(event)).push(handler); return next++; },
    "plugin:event|unlisten": () => null,
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) { const id = next++; cbs.set(id, cb); window[`_${id}`] = cb; return id; },
    unregisterCallback() {},
    convertFileSrc: (p) => p,
    invoke: async (cmd, args) => { const h = handlers[cmd]; return h ? h(args || {}) : null; },
  };
  window.__emit = (event, payload) => (listeners.get(event) || []).forEach((id) => cbs.get(id)?.({ event, id, payload }));
};

const meter = (l) => ({ peak_l: l, peak_r: l * 0.95, rms_l: l * 0.6, rms_r: l * 0.55 });

(async () => {
  require("node:fs").mkdirSync(OUT, { recursive: true });
  const browser = await chromium.launch();
  for (const [w, h] of SIZES) {
    for (const mode of MODES) {
      const page = await browser.newPage({ viewport: { width: w, height: h } });
      page.on("pageerror", (e) => console.log("pageerror", e.message));
      page.on("console", (m) => { if (m.type() === "error") console.log("console", m.text()); });
      await page.addInitScript(init);
      await page.goto(BASE);
      await page.waitForTimeout(600);
      await page.evaluate((mode) => {
        const trackA = { path: "/m/0.mp3", file_name: "0.mp3", title: "Reckless Behavior", artist: "Mad Tribe - Topic", album: null, duration_secs: 330, id: 1, bpm: 145, key: "6A", key_musical: "Gm" };
        const trackB = { path: "/m/1.mp3", file_name: "1.mp3", title: "LSD Party (Meltdown)", artist: "Mad Tribe - Topic", album: null, duration_secs: 330, id: 2, bpm: 148, key: "8A", key_musical: "Am" };
        window.__emit("track-loaded", { deck: 0, track: trackA, frames: 330 * 48000 });
        window.__emit("track-loaded", { deck: 1, track: trackB, frames: 330 * 48000 });
        const cue = (i, start, name) => ({ id: i, kind: { kind: "hot", index: i }, start, end: null, color: null, name });
        const slots = (off) => Array.from({ length: 8 }, (_, i) => (i < 5 ? cue(i, 8 + i * 31 + off, ["Intro", "Drop", "Break", "", "Outro"][i]) : null));
        window.__emit("hot-cues-changed", { deck: 0, slots: slots(0) });
        window.__emit("hot-cues-changed", { deck: 1, slots: slots(11) });
        window.__emit("pad-mode-changed", { deck: 0, mode: "hot_cue" });
        window.__emit("pad-mode-changed", { deck: 1, mode });
        let seq = 1;
        const t0 = performance.now();
        const push = () => {
          const t = (performance.now() - t0) / 1000;
          const deck = (pos, bpm, lead) => ({ loaded: true, playing: true, position: Math.floor(pos * 48000), duration: 330 * 48000, slip_position: null, rate: 1, bpm, beat_phase: (pos * bpm / 60) % 1, beat_in_bar: 0, sync_mode: "off", is_leader: lead, loop_region: null, loop_active: false, meter: { peak_l: 0.7 + 0.2 * Math.sin(t * 7), peak_r: 0.65 + 0.2 * Math.sin(t * 6), rms_l: 0.4, rms_r: 0.38 } });
          const empty = { loaded: false, playing: false, position: 0, duration: 0, slip_position: null, rate: 1, bpm: null, beat_phase: 0, beat_in_bar: 0, sync_mode: "off", is_leader: false, loop_region: null, loop_active: false, meter: { peak_l: 0, peak_r: 0, rms_l: 0, rms_r: 0 } };
          const m = { peak_l: 0.8, peak_r: 0.75, rms_l: 0.5, rms_r: 0.48 };
          const fx = { enabled: false, active: false, level: 0 };
          window.__emit("engine-telemetry", { sequence: seq++, engine_time: t, sample_rate: 48000, decks: [deck(113.6 + t, 145.0, true), deck(61 + t, 144.98, false), empty, empty], master: m, headphones: m, booth: m, inputs: { connected: false, mic: m, aux: m, duck_db: 0, underruns: 0, overruns: 0 }, fx_decks: [fx, fx, fx, fx], fx_master: fx, fx_sends: [fx, fx], sampler: Array.from({ length: 16 }, (_, i) => ({ loaded: i < 5, playing: i === 1, progress: 0.4 })), sampler_meter: m, leader: 0, dsp_load: 0.1, xruns: 0, dropped_events: 0 });
        };
        push();
        setInterval(push, 50);
      }, mode);
      await page.waitForTimeout(1200);
      const file = `${OUT}/${TAG}-${w}x${h}-${mode}.png`;
      await page.screenshot({ path: file });
      // Clipping check: any element wider/taller than its clipping ancestor chain.
      const report = await page.evaluate(() => {
        const bad = [];
        document.querySelectorAll("button,canvas,input,output,h3,p,span,[role=slider]").forEach((el) => {
          if (el.closest("[aria-hidden=true]") && el.tagName !== "CANVAS") return;
          const r = el.getBoundingClientRect();
          if (r.width === 0 || r.height === 0) return;
          let p = el.parentElement;
          while (p && p !== document.body) {
            const cs = getComputedStyle(p);
            if (/(hidden|auto|scroll|clip)/.test(cs.overflow + cs.overflowX + cs.overflowY)) {
              const pr = p.getBoundingClientRect();
              if (r.left < pr.left - 1 || r.right > pr.right + 1 || r.top < pr.top - 1 || r.bottom > pr.bottom + 1) {
                bad.push(`${el.tagName}.${(el.getAttribute("aria-label") || el.textContent || "").slice(0, 24)} in ${(p.getAttribute("aria-label") || p.className).toString().slice(0, 30)}`);
                break;
              }
            }
            p = p.parentElement;
          }
        });
        return { bad: [...new Set(bad)].slice(0, 30), scrollW: document.documentElement.scrollWidth, scrollH: document.documentElement.scrollHeight };
      });
      console.log(file, JSON.stringify(report));
      await page.close();
    }
  }
  await browser.close();
})();
