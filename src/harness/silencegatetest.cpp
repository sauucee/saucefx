/* Exercise the production silence gate with deterministic audio buffers. */
#include <stdio.h>
#include <string.h>

#include "silence_gate.h"

static int failures;

#define CHECK(cond, msg) do { \
	if (cond) { \
		printf("  PASS  %s\n", (msg)); \
	} else { \
		printf("  FAIL  %s\n", (msg)); \
		failures++; \
	} \
} while (0)

struct FakeProcessor {
	long tail_remaining;   /* nonzero output frames left to emit */
	int nonzero_forever;   /* model of a tail that never decays to 0.0f */
	long processed_blocks;
	long nonzero_blocks;
};

static int fake_process(struct FakeProcessor *dsp, float *buf, long count)
{
	long emit = 0;
	dsp->processed_blocks++;

	if (dsp->nonzero_forever) {
		for (long i = 0; i < count; i++)
			buf[i] = 1e-30f;
		dsp->nonzero_blocks++;
		return 1;
	}
	if (dsp->tail_remaining > 0) {
		emit = dsp->tail_remaining < count ? dsp->tail_remaining : count;
		for (long i = 0; i < emit; i++)
			buf[i] = 1e-6f;
		dsp->tail_remaining -= emit;
	}
	for (long i = emit; i < count; i++)
		buf[i] = 0.0f;
	if (emit > 0)
		dsp->nonzero_blocks++;
	return emit > 0;
}

/* Mirrors the proposed capture_process() ordering exactly. */
struct Harness {
	struct saucefx_silence_gate gate;
	struct FakeProcessor dsp;
	long frames;
	int channels;
	long processed;
	long skipped;
	int last_skipped;
	long last_frames;
};

static void harness_init(struct Harness *h, long frames, int channels, int rate)
{
	memset(h, 0, sizeof(*h));
	h->frames = frames;
	h->channels = channels;
	saucefx_gate_init(&h->gate, rate);
}

/* Returns 1 when the block was skipped (processor not called). */
static int harness_block(struct Harness *h, float wake_sample)
{
	float buf[512];
	long count = h->frames * h->channels;

	memset(buf, 0, sizeof(buf));
	if (wake_sample != 0.0f)
		buf[count - 1] = wake_sample;

	int in_zero = saucefx_gate_zero(buf, count);
	if (saucefx_gate_decision(&h->gate, in_zero, h->frames)) {
		h->skipped++;
		saucefx_gate_skipped(&h->gate, h->frames);
		h->last_skipped = 1;
		h->last_frames = count;
		return 1;
	}

	int out_nonzero = fake_process(&h->dsp, buf, count);
	h->processed++;
	saucefx_gate_processed(&h->gate, !out_nonzero, h->frames);
	h->last_skipped = 0;
	h->last_frames = count;
	return 0;
}

static void test_exact_zero_definition(void)
{
	float zeros[4] = { 0.0f, -0.0f, 0.0f, -0.0f };
	float subnormal[4] = { 0.0f, 0.0f, 0.0f, 1.4012984643e-45f };
	float quiet[4] = { 0.0f, 0.0f, 0.0f, 1e-30f };

	CHECK(saucefx_gate_zero(zeros, 4) == 1,
	      "zero: +0.0f and -0.0f are silent");
	CHECK(saucefx_gate_zero(subnormal, 4) == 0,
	      "zero: smallest positive subnormal is not silent");
	CHECK(saucefx_gate_zero(quiet, 4) == 0,
	      "zero: 1e-30f is not silent");
}

static void test_cold_idle_skips_after_both_windows(void)
{
	struct Harness h;
	long first_skip = 0;

	harness_init(&h, 256, 2, 48000);
	for (long i = 1; i <= 400; i++) {
		if (harness_block(&h, 0.0f) && first_skip == 0)
			first_skip = i;
	}
	CHECK(first_skip >= 94,
	      "cold: no skip before either 500 ms window could fill");
	CHECK(first_skip > 0 && first_skip <= 96,
	      "cold: skipping starts once both windows are full");
	CHECK(h.skipped > 0 && h.last_frames == 512,
	      "cold: skipped blocks keep the same frame count");
	CHECK(h.gate.in_run <= h.gate.in_hold &&
	      h.gate.out_run <= h.gate.out_hold,
	      "cold: guard counters are clamped to their window");
}

/*
 * A 3.75 s tail at 48 kHz (180000 frames) is longer than any fixed hold used
 * in the proposal: no block of it may be skipped, and the first skip must
 * come only after the output has been exactly zero for the whole output
 * window.
 */
static void test_tail_longer_than_any_hold_is_never_skipped(void)
{
	struct Harness h;
	long tail_blocks = (180000 + 255) / 256;
	long first_skip = 0;
	long skipped_during_tail = 0;
	long skipped_after_tail = 0;

	harness_init(&h, 256, 2, 48000);
	h.dsp.tail_remaining = 180000 * h.channels; /* interleaved samples */

	harness_block(&h, 0.25f);               /* the tone */
	CHECK(h.processed == 1 && h.dsp.nonzero_blocks == 1,
	      "tail: the tone block is processed and nonzero");

	for (long i = 2; i <= tail_blocks + 1; i++) {
		if (harness_block(&h, 0.0f)) {
			skipped_during_tail++;
			if (first_skip == 0)
				first_skip = i;
		}
	}
	CHECK(h.dsp.tail_remaining == 0,
	      "tail: the whole 3.75 s decay was fed to the processor");
	CHECK(skipped_during_tail == 0,
	      "tail: every nonzero-output block was processed and queued");
	CHECK(first_skip == 0,
	      "tail: nothing was skipped while output was nonzero");

	/* Output is now exact zero. The output window needs about 94 blocks,
	 * and the input window has been full since the first 500 ms of the
	 * tail, so skipping may begin only after that refill. */
	for (long i = tail_blocks + 2; i <= tail_blocks + 120; i++) {
		if (harness_block(&h, 0.0f)) {
			skipped_after_tail++;
			if (first_skip == 0)
				first_skip = i;
		}
	}
	CHECK(first_skip > tail_blocks + 90,
	      "tail: skipping waits for the full output guard window");
	CHECK(skipped_after_tail > 0,
	      "tail: skipping resumes after exact-zero output persists");
}

static void test_first_nonzero_input_wakes(void)
{
	struct Harness h;
	long skipped_before;
	long processed_before;

	harness_init(&h, 256, 2, 48000);
	for (long i = 0; i < 400; i++)
		harness_block(&h, 0.0f);
	CHECK(h.skipped > 0, "wake: precondition, gate is skipping");

	skipped_before = h.skipped;
	processed_before = h.processed;

	/* One subnormal sample in the last position of an otherwise zero
	 * block must process that whole block, not skip it. */
	harness_block(&h, 1.4012984643e-45f);
	CHECK(h.skipped == skipped_before && h.processed == processed_before + 1,
	      "wake: subnormal input wakes on its own block");
	CHECK(h.gate.skipping == 0 && h.gate.in_run == 0 &&
	      h.gate.out_run < h.gate.out_hold,
	      "wake: wake restarts both windows");

	/* The following zero block is processed because the output window
	 * has only one block of evidence, not a full window. */
	harness_block(&h, 0.0f);
	CHECK(h.skipped == skipped_before,
	      "wake: the block after the wake is processed too");
}

static void test_output_never_exact_zero_never_skips(void)
{
	struct Harness h;

	harness_init(&h, 256, 2, 48000);
	h.dsp.nonzero_forever = 1;
	for (long i = 0; i < 3000; i++)
		harness_block(&h, 0.0f);
	CHECK(h.skipped == 0,
	      "never-zero output: processor keeps running indefinitely");
	CHECK(h.gate.out_run == 0,
	      "never-zero output: output guard window stays empty");
}

static void test_reset_after_control_or_format_disarms_skip(void)
{
	struct Harness h;
	long skipped_before;

	harness_init(&h, 256, 2, 48000);
	for (long i = 0; i < 400; i++)
		harness_block(&h, 0.0f);
	CHECK(h.skipped > 0, "reset: precondition, gate is skipping");

	/* This is what invalidate_gate() plus saucefx_gate_reset() do after a
	 * preset/power/effect/EQ mutation: no immediate skip is allowed. */
	saucefx_gate_reset(&h.gate);
	skipped_before = h.skipped;
	CHECK(harness_block(&h, 0.0f) == 0,
	      "reset: first block after invalidation is processed");
	CHECK(h.skipped == skipped_before,
	      "reset: no skip on the invalidation block");

	/* A format change re-initialises the windows at the new rate. */
	saucefx_gate_init(&h.gate, 44100);
	CHECK(harness_block(&h, 0.0f) == 0,
	      "reset: new format is processed, never skipped");
}

static void test_unknown_rate_fails_open(void)
{
	struct Harness h;

	harness_init(&h, 256, 2, 0);
	for (long i = 0; i < 400; i++)
		harness_block(&h, 0.0f);
	CHECK(h.skipped == 0,
	      "unknown rate: helper never authorises skipping");
	CHECK(h.gate.in_hold == 0 && h.gate.out_hold == 0,
	      "unknown rate: windows are empty, so nothing can fill");
}

static void test_counters_stay_bounded(void)
{
	struct Harness h;

	harness_init(&h, 256, 2, 48000);
	h.dsp.nonzero_forever = 0;
	for (long i = 0; i < 50000; i++)
		harness_block(&h, 0.0f);   /* 12.8 M frames of idle */
	CHECK(h.gate.in_run == h.gate.in_hold &&
	      h.gate.out_run == h.gate.out_hold,
	      "counters: both clamp at their window after long idle");
	CHECK(h.gate.in_hold == 24000 && h.gate.out_hold == 24000,
	      "counters: 500 ms windows at 48 kHz are 24000 frames");
}

int main(void)
{
	printf("behaviour: shared silence-gate helper\n");
	test_exact_zero_definition();
	test_cold_idle_skips_after_both_windows();
	test_tail_longer_than_any_hold_is_never_skipped();
	test_first_nonzero_input_wakes();
	test_output_never_exact_zero_never_skips();
	test_reset_after_control_or_format_disarms_skip();
	test_unknown_rate_fails_open();
	test_counters_stay_bounded();

	if (failures) {
		printf("behaviour: %d check(s) failed\n", failures);
		return 1;
	}
	printf("behaviour: all checks passed\n");
	return 0;
}
