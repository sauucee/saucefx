import { useEffect, useRef, useState } from 'react';
import { Mic, MicOff } from 'lucide-react';
import { getSignalEngine } from '../lib/useSignal';
import { usePrefersReducedMotion } from '../lib/usePrefersReducedMotion';

type MicState = 'off' | 'requesting' | 'live' | 'denied';

/**
 * Opt-in mic bridge: real FFT frames -> the same signal engine the hero
 * canvas already reads. Never auto-starts; reduced motion hides it.
 */
export function SignalToggle() {
  const reduced = usePrefersReducedMotion();
  const [state, setState] = useState<MicState>('off');
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => () => cleanupRef.current?.(), []);

  const start = async () => {
    setState('requesting');
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const ctx = new AudioContext();
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 4096;
      analyser.smoothingTimeConstant = 0.8;
      ctx.createMediaStreamSource(stream).connect(analyser);
      const fft = new Float32Array(analyser.frequencyBinCount);
      const nyquist = ctx.sampleRate / 2;
      const bin = (hz: number) =>
        Math.min(fft.length - 1, Math.round((hz / nyquist) * fft.length));
      // 31 log-spaced bands, 40 Hz – 16 kHz
      const edges = Array.from({ length: 32 }, (_, i) => bin(40 * (16000 / 40) ** (i / 31)));
      const engine = getSignalEngine();
      engine.setSource((target) => {
        analyser.getFloatFrequencyData(fft);
        // Float data is dBFS, typically ~-100..-30 dB. Map -85..-15 dB -> 0..1
        // (narrowed range amplifies visual contrast for typical listening levels).
        for (let i = 0; i < target.length; i++) {
          let peak = -120;
          for (let j = edges[i]; j <= edges[i + 1]; j++) peak = Math.max(peak, fft[j]);
          target[i] = Math.min(1, Math.max(0, (peak + 85) / 70));
        }
      });
      engine.mode = 'mic';
      cleanupRef.current = () => {
        engine.mode = 'demo';
        void ctx.close();
        stream.getTracks().forEach((track) => track.stop());
        cleanupRef.current = null;
      };
      setState('live');
    } catch {
      setState('denied');
    }
  };

  if (reduced) return null;

  const stop = () => {
    cleanupRef.current?.();
    setState('off');
  };

  const label: Record<MicState, string> = {
    off: 'Drive the visuals with your mic',
    requesting: 'Waiting for mic permission…',
    live: 'Listening — tap to stop',
    denied: 'Mic blocked — tap to retry',
  };

  return (
    <button
      type="button"
      className="signal-toggle label"
      onClick={state === 'live' ? stop : start}
      aria-pressed={state === 'live'}
    >
      {state === 'live' ? <MicOff size={13} aria-hidden /> : <Mic size={13} aria-hidden />}
      {label[state]}
    </button>
  );
}
