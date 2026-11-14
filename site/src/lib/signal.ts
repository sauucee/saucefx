export type SignalSource = (target: Float32Array, t: number) => void;

export type SignalEngine = {
  readonly bands: Float32Array; // 31 eased values, 0..1
  readonly level: number; // 0..1 mean energy
  /** Seconds of signal elapsed since creation (for the readout timecode). */
  readonly time: number;
  mode: 'demo' | 'mic';
  /** 0 = flatline (the narrative "before"), 1 = full signal. */
  gain: number;
  setSource(next: SignalSource | null): void;
  tick(dt: number): void;
};

/** One engine instance feeds shader + floor + DOM; demo signal is deterministic. */
export function createSignalEngine(bandCount = 31): SignalEngine {
  const bands = new Float32Array(bandCount);
  const target = new Float32Array(bandCount);
  let t = 0;
  let level = 0;
  let mode: 'demo' | 'mic' = 'demo';
  let gain = 1;
  let source: SignalSource | null = null;

  return {
    bands,
    get level() {
      return level;
    },
    get time() {
      return t;
    },
    get mode() {
      return mode;
    },
    set mode(next) {
      mode = next;
      if (next === 'demo') source = null;
    },
    get gain() {
      return gain;
    },
    set gain(next) {
      gain = Math.min(1, Math.max(0, next));
    },
    setSource(next) {
      source = next;
    },
    tick(dt) {
      t += Math.min(dt, 0.05);
      if (source) {
        source(target, t);
      } else {
        // Demo "track": shaped partials + a heartbeat in the lows. No audio input.
        for (let i = 0; i < bandCount; i++) {
          const n = i / (bandCount - 1); // 0 = lows, 1 = highs
          const sway = 0.5 + 0.5 * Math.sin(t * (0.4 + n * 1.1) + i * 0.63);
          const kick = Math.exp(-((t % 0.55) * 8)) * (1 - n) * 0.85;
          const air = 0.1 * Math.sin(t * 2.3 + i * 1.31) ** 2;
          target[i] = Math.min(
            1,
            0.12 + 0.66 * sway ** 1.7 * (0.4 + 0.6 * (1 - n)) + kick + air,
          );
        }
      }
      const ease = Math.min(1, dt * 14);
      let sum = 0;
      for (let i = 0; i < bandCount; i++) {
        bands[i] += (target[i] * gain - bands[i]) * ease;
        sum += bands[i];
      }
      level += (sum / bandCount - level) * Math.min(1, dt * 8);
    },
  };
}
