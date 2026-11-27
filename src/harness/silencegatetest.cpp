/* Exercise the production silence gate with deterministic audio buffers.
 *
 * The processor writes the same in-place float buffer the real bridge uses;
 * the gate reads that buffer back. No test trusts a boolean stand-in for the
 * post-process output.
 */
#include <math.h>
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

#define RATE 48000
#define FRAMES 256
#define CHANNELS 2
#define COUNT (FRAMES * CHANNELS)

/* The real engine holds tiny filter biases even on digital-zero input. */
#define BIAS_A 3.1477e-25f
#define BIAS_B 3.1401e-25f
#define ABOVE_FLOOR 1e-19f

/* 4000 ms input settle at 48 kHz / 256 frames = 750 blocks; the independent
 * 500 ms output residual is 94 blocks. Loops run past both. */
#define INPUT_SETTLE_BLOCKS 750
#define IDLE_BLOCKS 1000

struct FakeProcessor {
	long tail_samples;   /* 1e-6f output samples left (interleaved) */
	float bias;          /* held output after any tail */
	int nonfinite;       /* 0 none, 1 NaN, 2 +Inf, 3 -Inf */
	long processed_blocks;
};

static void fake_process(struct FakeProcessor *dsp, float *buf, long count)
{
	long emit = 0;

	dsp->processed_blocks++;
	if (dsp->nonfinite) {
		float v = dsp->nonfinite == 1 ? NAN :
			dsp->nonfinite == 2 ? INFINITY : -INFINITY;
		for (long i = 0; i < count; i++)
			buf[i] = v;
		return;
	}
	if (dsp->tail_samples > 0) {
		emit = dsp->tail_samples < count ? dsp->tail_samples : count;
		for (long i = 0; i < emit; i++)
			buf[i] = 1e-6f;
		dsp->tail_samples -= emit;
	}
	for (long i = emit; i < count; i++)
		buf[i] = dsp->bias;
}

/* Mirrors the production capture_process() ordering. */
struct Harness {
	struct saucefx_silence_gate gate;
	struct FakeProcessor dsp;
	long processed;
	long skipped;
	int last_skipped;
	long last_frames;
	float last_peak;              /* last emitted block (skipped = 0) */
	int last_exact_zero;
	float last_processed_peak;    /* last block the processor ran on */
	int last_processed_exact_zero;
};

static void harness_init(struct Harness *h, int rate)
{
	memset(h, 0, sizeof(*h));
	saucefx_gate_init(&h->gate, rate);
}

static void harness_measure_output(struct Harness *h, const float *buf,
				   long count)
{
	int exact_zero = saucefx_gate_zero(buf, count);

	h->last_exact_zero = exact_zero;
	h->last_peak = 0.0f;
	for (long i = 0; i < count; i++) {
		float v = fabsf(buf[i]);
		if (v > h->last_peak)
			h->last_peak = v;
	}
}

/* Returns 1 when the block was skipped (processor not called). */
static int harness_block(struct Harness *h, float wake_sample)
{
	float buf[COUNT];

	memset(buf, 0, sizeof(buf));
	if (wake_sample != 0.0f)
		buf[COUNT - 1] = wake_sample;

	int in_zero = saucefx_gate_zero(buf, COUNT);
	if (saucefx_gate_decision(&h->gate, in_zero, FRAMES)) {
		h->skipped++;
		h->last_skipped = 1;
		h->last_frames = COUNT;
		harness_measure_output(h, buf, COUNT);
		saucefx_gate_skipped(&h->gate, FRAMES);
		return 1;
	}

	fake_process(&h->dsp, buf, COUNT);
	h->processed++;
	harness_measure_output(h, buf, COUNT);
	h->last_processed_peak = h->last_peak;
	h->last_processed_exact_zero = h->last_exact_zero;
	saucefx_gate_processed(&h->gate,
			       saucefx_gate_output_silent(buf, COUNT), FRAMES);
	h->last_skipped = 0;
	h->last_frames = COUNT;
	return 0;
}

static void test_input_exact_zero_rule(void)
{
	float zeros[4] = { 0.0f, -0.0f, 0.0f, -0.0f };
	float subnormal[4] = { 0.0f, 0.0f, 0.0f, 1.4012984643e-45f };
	float quiet[4] = { 0.0f, 0.0f, 0.0f, 1e-30f };

	CHECK(saucefx_gate_zero(zeros, 4) == 1,
	      "input: +0.0f and -0.0f are exact-zero input");
	CHECK(saucefx_gate_zero(subnormal, 4) == 0,
	      "input: smallest positive subnormal is not zero");
	CHECK(saucefx_gate_zero(quiet, 4) == 0,
	      "input: 1e-30f is not zero");
}

static void test_output_floor_definition(void)
{
	float zeros[4] = { 0.0f, -0.0f, 0.0f, -0.0f };
	float bias[4] = { BIAS_A, -BIAS_B, 3.0e-25f, -1.0e-25f };
	float just_below[4] = { 9.999e-21f, -9.999e-21f, 0.0f, 0.0f };
	float at_floor[4] = { 1e-20f, 0.0f, 0.0f, 0.0f };
	float at_neg_floor[4] = { -1e-20f, 0.0f, 0.0f, 0.0f };
	float above[4] = { 0.0f, ABOVE_FLOOR, 0.0f, 0.0f };
	float nan_buf[4] = { 0.0f, 0.0f, 0.0f, NAN };
	float inf_buf[4] = { 0.0f, INFINITY, 0.0f, 0.0f };
	float ninf_buf[4] = { -INFINITY, 0.0f, 0.0f, 0.0f };

	CHECK(saucefx_gate_output_silent(zeros, 4) == 1,
	      "output: exact zeros are silent");
	CHECK(saucefx_gate_output_silent(bias, 4) == 1,
	      "output: measured 3.1e-25 filter bias is silent");
	CHECK(saucefx_gate_output_silent(just_below, 4) == 1,
	      "output: 9.999e-21 is below the floor");
	CHECK(saucefx_gate_output_silent(at_floor, 4) == 0,
	      "output: exactly +1e-20 is not silent");
	CHECK(saucefx_gate_output_silent(at_neg_floor, 4) == 0,
	      "output: exactly -1e-20 is not silent");
	CHECK(saucefx_gate_output_silent(above, 4) == 0,
	      "output: 1e-19 amplitude is not silent");
	CHECK(saucefx_gate_output_silent(nan_buf, 4) == 0,
	      "output: NaN can never look silent");
	CHECK(saucefx_gate_output_silent(inf_buf, 4) == 0,
	      "output: +Inf can never look silent");
	CHECK(saucefx_gate_output_silent(ninf_buf, 4) == 0,
	      "output: -Inf can never look silent");
}

static void test_bias_on_silent_input_eventually_skips(void)
{
	const float biases[] = { BIAS_A, -BIAS_B };
	long first_skip = 0;
	struct Harness h;
	int i;

	for (i = 0; i < 2; i++) {
		harness_init(&h, RATE);
		h.dsp.bias = biases[i];
		first_skip = 0;
		for (long b = 1; b <= IDLE_BLOCKS; b++) {
			if (harness_block(&h, 0.0f) && first_skip == 0)
				first_skip = b;
		}
		CHECK(first_skip == INPUT_SETTLE_BLOCKS,
		      biases[i] > 0.0f ?
		      "bias +3.1477e-25: skips exactly at the 4 s input settle" :
		      "bias -3.1401e-25: skips exactly at the 4 s input settle");
		CHECK(h.skipped > 0 && h.last_frames == COUNT,
		      "bias: skipped blocks keep the same frame count");
		CHECK(!h.last_processed_exact_zero &&
		      h.last_processed_peak < 1e-20f &&
		      h.last_processed_peak > 0.0f,
		      "bias: processed output is below floor but never zero");
	}
}

/*
 * Quiet-wake regression: with below-floor output the engine is processed for
 * the entire 4 s input settle, then genuinely sleeps.
 */
static void test_quiet_engine_settles_before_sleep(void)
{
	struct Harness h;
	long early_skips = 0;

	harness_init(&h, RATE);
	h.dsp.bias = BIAS_A;
	for (long b = 1; b < INPUT_SETTLE_BLOCKS; b++)
		if (harness_block(&h, 0.0f))
			early_skips++;
	CHECK(early_skips == 0 && h.processed == INPUT_SETTLE_BLOCKS - 1,
	      "settle: quiet engine processed for the first 3.99 s");
	CHECK(harness_block(&h, 0.0f) == 1,
	      "settle: skipping starts exactly at the 4 s input guard");
	CHECK(harness_block(&h, 0.0f) == 1 && h.skipped >= 2,
	      "settle: gate genuinely skips after the input guard");
}

/*
 * The input settle and output residual guards are independent and scale with
 * the negotiated rate. The residual is full long before the settle ends, so
 * the skip point follows the 4 s input guard at each rate.
 */
static void test_guards_scale_with_rate(void)
{
	struct saucefx_silence_gate g;
	struct Harness h;
	long first_skip = 0;

	saucefx_gate_init(&g, 48000);
	CHECK(g.in_hold == 192000 && g.out_hold == 24000,
	      "rate: 48 kHz = 4000 ms input settle, 500 ms output residual");
	saucefx_gate_init(&g, 44100);
	CHECK(g.in_hold == 176400 && g.out_hold == 22050,
	      "rate: 44.1 kHz guards scale with the rate");
	saucefx_gate_init(&g, 96000);
	CHECK(g.in_hold == 384000 && g.out_hold == 48000,
	      "rate: 96 kHz guards scale with the rate");

	harness_init(&h, 44100);
	h.dsp.bias = BIAS_A;
	for (long b = 1; b <= IDLE_BLOCKS; b++) {
		if (harness_block(&h, 0.0f) && first_skip == 0)
			first_skip = b;
	}
	CHECK(first_skip == 690,
	      "rate: 44.1 kHz skips at its own 4 s settle, not 48 kHz's 750");
}

static void test_output_above_floor_never_skips(void)
{
	struct Harness h;

	harness_init(&h, RATE);
	h.dsp.bias = ABOVE_FLOOR;
	for (long b = 0; b < IDLE_BLOCKS; b++)   /* 5.3 s, past both guards */
		harness_block(&h, 0.0f);
	CHECK(h.skipped == 0,
	      "above floor: continuous 1e-19 output never skips after 5.3 s");
	CHECK(h.gate.out_run == 0,
	      "above floor: output guard window stays empty");
	CHECK(h.last_peak >= ABOVE_FLOOR,
	      "above floor: harness observed real above-floor output");
}

static void test_nonfinite_output_never_skips(void)
{
	struct Harness h;
	int kind;

	for (kind = 1; kind <= 3; kind++) {
		harness_init(&h, RATE);
		h.dsp.nonfinite = kind;
		for (long b = 0; b < IDLE_BLOCKS; b++)
			harness_block(&h, 0.0f);
		CHECK(h.skipped == 0,
		      kind == 1 ? "nonfinite: NaN output never skips" :
		      kind == 2 ? "nonfinite: +Inf output never skips" :
		      "nonfinite: -Inf output never skips");
		CHECK(h.gate.out_run == 0,
		      "nonfinite: output window stays empty");
	}
}

/*
 * A 5.25 s continuous tail is longer than the 4 s input settle guard: the
 * expired input guard must never cut it. Every block whose real output is
 * nonzero is processed and queued, and skipping may only follow the
 * independent 500 ms output residual.
 */
static void test_tail_longer_than_input_settle_is_never_skipped(void)
{
	struct Harness h;
	const long tail_samples = 252000 * CHANNELS;   /* 5.25 s interleaved */
	const long tail_blocks = (tail_samples + COUNT - 1) / COUNT;
	long first_skip = 0;
	long skipped_during_tail = 0;
	long skipped_after_tail = 0;

	harness_init(&h, RATE);
	h.dsp.tail_samples = tail_samples;

	harness_block(&h, 0.25f);   /* the tone */
	CHECK(h.processed == 1 && h.last_peak > 0.0f,
	      "tail: the tone block is processed and audible");

	for (long b = 2; b <= tail_blocks + 1; b++) {
		if (harness_block(&h, 0.0f)) {
			skipped_during_tail++;
			if (first_skip == 0)
				first_skip = b;
		}
	}
	CHECK(h.dsp.tail_samples == 0,
	      "tail: the whole 5.25 s decay was fed to the processor");
	CHECK(skipped_during_tail == 0,
	      "tail: every nonzero-output block was processed and queued");
	CHECK(first_skip == 0,
	      "tail: nothing was skipped while output was audible");

	for (long b = tail_blocks + 2; b <= tail_blocks + 120; b++) {
		if (harness_block(&h, 0.0f)) {
			skipped_after_tail++;
			if (first_skip == 0)
				first_skip = b;
		}
	}
	CHECK(first_skip > tail_blocks + 90,
	      "tail: skipping waits for the 500 ms output residual");
	CHECK(skipped_after_tail > 0,
	      "tail: skipping resumes once output stays below floor");
}

/* Every first-nonzero input wakes on its own block, however it is encoded. */
static void test_first_nonzero_input_wakes(void)
{
	const float wakes[] = { 1.4012984643e-45f, 1e-30f, NAN,
				INFINITY, -INFINITY };
	const char *names[] = { "subnormal", "1e-30", "NaN", "+Inf", "-Inf" };
	struct Harness h;
	int i;

	for (i = 0; i < 5; i++) {
		long skipped_before;
		long processed_before;

		harness_init(&h, RATE);
		for (long b = 0; b < IDLE_BLOCKS; b++)
			harness_block(&h, 0.0f);
		if (h.skipped == 0) {
			CHECK(0, "wake: precondition, gate is skipping");
			continue;
		}
		skipped_before = h.skipped;
		processed_before = h.processed;

		harness_block(&h, wakes[i]);
		{
			char message[96];
			snprintf(message, sizeof(message),
				 "wake: %s input wakes on its own block",
				 names[i]);
			CHECK(h.skipped == skipped_before &&
			      h.processed == processed_before + 1,
			      message);
		}
		CHECK(h.gate.skipping == 0 && h.gate.in_run == 0 &&
		      h.gate.out_run < h.gate.out_hold,
		      "wake: wake restarts both guard windows");
		harness_block(&h, 0.0f);
		CHECK(h.skipped == skipped_before,
		      "wake: the following block is processed too");
	}
}

static void test_skipped_output_framing_is_unchanged(void)
{
	struct Harness h;

	harness_init(&h, RATE);
	for (long b = 0; b < IDLE_BLOCKS; b++)
		harness_block(&h, 0.0f);
	CHECK(h.last_frames == COUNT,
	      "framing: skipped blocks keep the production frame count");
}

static void test_reset_after_control_or_format_disarms_skip(void)
{
	struct Harness h;
	long skipped_before;

	harness_init(&h, RATE);
	for (long b = 0; b < IDLE_BLOCKS; b++)
		harness_block(&h, 0.0f);
	CHECK(h.skipped > 0, "reset: precondition, gate is skipping");

	/* Production invalidation after a preset/power/effect/EQ mutation. */
	saucefx_gate_reset(&h.gate);
	skipped_before = h.skipped;
	CHECK(harness_block(&h, 0.0f) == 0,
	      "reset: first block after invalidation is processed");
	CHECK(h.skipped == skipped_before,
	      "reset: no skip on the invalidation block");

	/* Production format change re-initialises both guards at the new rate. */
	saucefx_gate_init(&h.gate, 44100);
	CHECK(h.gate.in_hold == 176400 && h.gate.out_hold == 22050,
	      "reset: format change rescales the 4 s/500 ms guards");
	CHECK(harness_block(&h, 0.0f) == 0,
	      "reset: new format is processed, never skipped");
}

static void test_unknown_rate_fails_open(void)
{
	struct Harness h;

	harness_init(&h, 0);
	for (long b = 0; b < IDLE_BLOCKS; b++)
		harness_block(&h, 0.0f);
	CHECK(h.skipped == 0,
	      "unknown rate: helper never authorises skipping");
	CHECK(h.gate.in_hold == 0 && h.gate.out_hold == 0,
	      "unknown rate: windows are empty, so nothing can fill");
}

static void test_counters_stay_bounded(void)
{
	struct Harness h;

	harness_init(&h, RATE);
	for (long b = 0; b < 50000; b++)   /* 12.8 M frames of idle */
		harness_block(&h, 0.0f);
	CHECK(h.gate.in_run == h.gate.in_hold &&
	      h.gate.out_run == h.gate.out_hold,
	      "counters: both clamp at their window after long idle");
	CHECK(h.gate.in_hold == 192000 && h.gate.out_hold == 24000,
	      "counters: 4000 ms input settle and 500 ms output residual");
}

int main(void)
{
	printf("behaviour: silence gate with real post-process buffers\n");
	test_input_exact_zero_rule();
	test_output_floor_definition();
	test_bias_on_silent_input_eventually_skips();
	test_quiet_engine_settles_before_sleep();
	test_guards_scale_with_rate();
	test_output_above_floor_never_skips();
	test_nonfinite_output_never_skips();
	test_tail_longer_than_input_settle_is_never_skipped();
	test_first_nonzero_input_wakes();
	test_skipped_output_framing_is_unchanged();
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
