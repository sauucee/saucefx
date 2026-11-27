/*
 * silence_gate.h - conservative decision helper for skipping DfxDsp work on
 * digital silence. Header-only: no allocation, locks, I/O or platform calls.
 *
 * The decision needs evidence from both sides of the processor, because the
 * input can be silent while the engine is still emitting an effect tail:
 *
 *   - in_run  counts consecutive frames of exact-zero input and must reach
 *     the input settle guard (4000 ms below);
 *   - out_run counts consecutive frames of post-process output that is
 *     finite and strictly inside the output floor and must reach the
 *     independent output residual guard (500 ms below);
 *   - both guards must be full before a block may be skipped.
 *
 * The input settle guard is deliberately four seconds, not a tail estimate.
 * CI found that Voice, Gaming and Movies wake quietly but need up to four
 * seconds of exact-zero-input processing before their output matches the
 * always-processing reference; a 0.5, 1 or 2 second guard did not. It
 * applies on startup, on resumed nonzero input, and after any format or
 * settings invalidation. The output residual guard stays short because it
 * follows the processor's actual output rather than a timer.
 *
 * processAudio() runs in place, so the caller re-scans the same buffer after
 * the call. Any nonzero input sample restarts the input window. Any output
 * sample at or above the floor, or any NaN/Inf, restarts the output window,
 * so every block of an audible decay is processed and queued. If the output
 * never falls below the floor, out_run stays empty and the processor keeps
 * running indefinitely.
 *
 * The input rule stays exact: a block is only ever a skip candidate when
 * every input sample is exactly +/-0.0f. A nonzero input sample always wins,
 * regardless of magnitude (including subnormals), so the first audible
 * sample is never lost - even one below the output floor.
 *
 * The floor applies only to the processor's own output. The core DSP holds
 * deliberate tiny filter biases (measured peaks 3.1477e-25 and 3.1401e-25
 * on digital-zero input with the Music preset), which never reach exact
 * zero; dfxpUniversal.cpp documents the same 1e-20f silence convention.
 */
#ifndef SAUCEFX_SILENCE_GATE_H
#define SAUCEFX_SILENCE_GATE_H

#include <stddef.h>

/* Two independent guards, both scaled by the negotiated rate and zero when
 * the rate is unknown (fail open).
 *
 * INPUT (settle): exact-zero input must persist for 4000 ms. This is the
 * minimum processing time on startup, on resumed signal, and after format or
 * settings invalidation, so a quiet engine's settling transient is fully
 * processed before any sleep.
 *
 * OUTPUT (residual): post-process output must stay finite and strictly
 * inside the output floor for 500 ms. It tracks the real residual and is
 * independent of the input settle guard.
 */
#define SAUCEFX_GATE_INPUT_MS 4000
#define SAUCEFX_GATE_OUTPUT_MS 500

/* Output-only absolute floor. Input remains an exact-zero test; this floor
 * only decides when the processor's residual is quiet enough to sleep.
 * Silence requires |sample| strictly below the floor, so a sample exactly at
 * +/-1e-20f keeps processing. */
#define SAUCEFX_GATE_OUTPUT_FLOOR 1e-20f

struct saucefx_silence_gate {
	long long in_run;    /* consecutive exact-zero input frames */
	long long out_run;   /* consecutive below-floor output frames */
	long long in_hold;   /* input settle guard in frames (4000 ms) */
	long long out_hold;  /* output residual guard in frames (500 ms) */
	int skipping;
};

/* Frames in one guard window at this rate; zero if unknown (fail open). */
static inline long long saucefx_gate_hold(long long rate, int ms)
{
	if (rate <= 0)
		return 0;
	return rate * (long long)ms / 1000;
}

static inline void saucefx_gate_init(struct saucefx_silence_gate *gate,
				     long long rate)
{
	gate->in_run = 0;
	gate->out_run = 0;
	gate->in_hold = saucefx_gate_hold(rate, SAUCEFX_GATE_INPUT_MS);
	gate->out_hold = saucefx_gate_hold(rate, SAUCEFX_GATE_OUTPUT_MS);
	gate->skipping = 0;
}

/* Re-arm after a control mutation or format change: the 4 s input settle
 * and the independent 500 ms output residual guard both restart. */
static inline void saucefx_gate_reset(struct saucefx_silence_gate *gate)
{
	gate->in_run = 0;
	gate->out_run = 0;
	gate->skipping = 0;
}

/* 1 when every sample is exactly +0.0f or -0.0f. No threshold, no tolerance. */
static inline int saucefx_gate_zero(const float *samples, long count)
{
	for (long i = 0; i < count; i++)
		if (samples[i] != 0.0f)
			return 0;
	return 1;
}

/*
 * 1 when every post-process sample is strictly inside the output floor. The
 * bounds are ordered comparisons (not fabs or bit manipulation), so they use
 * the shipped build's ordinary IEEE semantics, without fast-math:
 *
 *   - NaN fails both comparisons, so !(v > -floor && v < floor) is true;
 *   - +Inf fails v < floor and -Inf fails v > -floor, so both are rejected;
 *   - +/-0.0f pass, and a sample exactly at the floor is not silent.
 *
 * Non-finite output therefore always keeps the processor awake.
 */
static inline int saucefx_gate_output_silent(const float *samples, long count)
{
	for (long i = 0; i < count; i++) {
		float v = samples[i];

		if (!(v > -SAUCEFX_GATE_OUTPUT_FLOOR &&
		      v < SAUCEFX_GATE_OUTPUT_FLOOR))
			return 0;
	}
	return 1;
}

/* Bounded counter: never exceeds its window, so no overflow and no growth
 * proportional to idle time. */
static inline long long saucefx_gate_bump(long long value, long long add,
					  long long limit)
{
	if (limit <= 0)
		return 0;
	if (value >= limit)
		return limit;
	return value + add < limit ? value + add : limit;
}

/*
 * Decide whether processAudio() may be skipped for this block.
 * in_zero: the current input contains no nonzero sample.
 * Returns 1 only when the input settle guard and the output residual guard
 * are both already full; in every other case the caller must process the
 * block. A nonzero input always returns 0.
 */
static inline int saucefx_gate_decision(struct saucefx_silence_gate *gate,
					int in_zero, long long frames)
{
	/* Unknown rate or an uninitialised window fails open. */
	if (gate->in_hold <= 0 || gate->out_hold <= 0) {
		gate->skipping = 0;
		return 0;
	}

	if (!in_zero) {
		gate->in_run = 0;
		/* This block's output is unknown; do not let a stale zero
		 * output window authorise skipping the next block. */
		gate->out_run = 0;
		gate->skipping = 0;
		return 0;
	}

	gate->in_run = saucefx_gate_bump(gate->in_run, frames, gate->in_hold);
	if (gate->in_run < gate->in_hold || gate->out_run < gate->out_hold) {
		gate->skipping = 0;
		return 0;
	}

	gate->skipping = 1;
	return 1;
}

/*
 * Record the post-process output of the block that just ran. Output outside
 * the floor (or non-finite) restarts the output window, so the next block is
 * processed even when the input has already gone silent.
 */
static inline void saucefx_gate_processed(struct saucefx_silence_gate *gate,
					  int out_silent, long long frames)
{
	if (out_silent)
		gate->out_run = saucefx_gate_bump(gate->out_run, frames,
						  gate->out_hold);
	else
		gate->out_run = 0;
	gate->skipping = 0;
}

/* A skipped block is emitted as the exact-zero input, so it extends the
 * output window without running the processor. */
static inline void saucefx_gate_skipped(struct saucefx_silence_gate *gate,
					long long frames)
{
	gate->out_run = saucefx_gate_bump(gate->out_run, frames,
					  gate->out_hold);
}

#endif /* SAUCEFX_SILENCE_GATE_H */
