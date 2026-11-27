/*
 * silence_gate.h - conservative decision helper for skipping DfxDsp work on
 * digital silence. Header-only: no allocation, locks, I/O or platform calls.
 *
 * The decision needs evidence from both sides of the processor, because the
 * input can be silent while the engine is still emitting an effect tail:
 *
 *   - in_run  counts consecutive frames of exact-zero input;
 *   - out_run counts consecutive frames of exact-zero post-process output;
 *   - both windows must be full before a block may be skipped.
 *
 * processAudio() runs in place, so the caller re-scans the same buffer after
 * the call. Any nonzero sample anywhere in the input or the output restarts
 * the relevant window, so every block of a decay tail is processed and
 * queued. If the output never becomes exactly zero, out_run stays empty and
 * the processor keeps running indefinitely.
 *
 * A nonzero input sample always wins, regardless of magnitude (including
 * subnormals): the block it appears in is processed, not skipped.
 *
 * No amplitude threshold is used anywhere; "silent" means every float is
 * exactly +/-0.0f.
 */
#ifndef SAUCEFX_SILENCE_GATE_H
#define SAUCEFX_SILENCE_GATE_H

#include <stddef.h>

/* Guard windows in milliseconds of continuous silence. Both are reset by any
 * change to engine state or format, so they must refill before skipping. */
#define SAUCEFX_GATE_INPUT_MS 500
#define SAUCEFX_GATE_OUTPUT_MS 500

struct saucefx_silence_gate {
	long long in_run;
	long long out_run;
	long long in_hold;
	long long out_hold;
	int skipping;
};

/* Frames in a window; zero when the rate is unknown (fail open). */
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

/* Re-arm after a control mutation or format change: both windows restart. */
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
 * Returns 1 only when both guard windows are already full; in every other
 * case the caller must process the block. A nonzero input always returns 0.
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
 * Record the post-process output of the block that just ran. A nonzero output
 * restarts the output window, so the next block is processed even when the
 * input has already gone silent.
 */
static inline void saucefx_gate_processed(struct saucefx_silence_gate *gate,
					  int out_zero, long long frames)
{
	if (out_zero)
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
