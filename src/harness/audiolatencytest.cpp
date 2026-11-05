/* Exercise the actual private audio callbacks with synthetic PipeWire buffers.
 * Only dequeue/queue are replaced: no daemon, device or desktop routing is used. */
#include "../pwbridge/fxpw.cpp"

#include <stdexcept>
#include <vector>

static pw_buffer *test_buffer;

extern "C" pw_buffer *pw_stream_dequeue_buffer(pw_stream *)
{
	return test_buffer;
}

extern "C" int pw_stream_queue_buffer(pw_stream *, pw_buffer *buffer)
{
	if (buffer != test_buffer)
		throw std::runtime_error("queued the wrong buffer");
	return 0;
}

static void require(bool condition, const char *message)
{
	if (!condition)
		throw std::runtime_error(message);
}

struct Buffer {
	std::vector<float> samples;
	spa_chunk chunk{};
	spa_data data{};
	spa_buffer spa{};
	pw_buffer pw{};
	spa_io_rate_match rate_match{};

	void request(uint32_t frames)
	{
#if PW_CHECK_VERSION(0, 3, 50)
		pw.requested = frames;
#endif
		rate_match.size = frames;
	}

	Buffer(FXPW &engine, size_t capacity, uint32_t requested) : samples(capacity * 2, -1.0f)
	{
		data.data = samples.data();
		data.maxsize = samples.size() * sizeof(float);
		data.chunk = &chunk;
		spa.n_datas = 1;
		spa.datas = &data;
		pw.buffer = &spa;
		request(requested);
		render_io_changed(&engine, SPA_IO_RateMatch, &rate_match, sizeof(rate_match));
		test_buffer = &pw;
	}
};

static std::vector<float> ramp(size_t frames)
{
	std::vector<float> samples(frames * 2);
	for (size_t i = 0; i < frames; ++i) {
		samples[i * 2] = static_cast<float>(i);
		samples[i * 2 + 1] = -static_cast<float>(i);
	}
	return samples;
}

static void requestedFrames()
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = 48000;
	auto input = ramp(512);
	ring_push(&engine.ring, input.data(), 512);
	Buffer output(engine, 8192, 256);
	render_process(&engine);
	require(output.chunk.size == 256 * 2 * sizeof(float),
		"render must submit the requested frames, not buffer capacity");
	require(output.pw.size == 256, "queued frame count must be reported to PipeWire");
	require(ring_avail(&engine.ring) == 256, "normal queued audio must not be discarded");
	require(output.samples[510] == 255 && output.samples[511] == -255,
		"normal stereo audio must stay in order");
	require(output.samples[512] == -1.0f, "render must not write beyond requested frames");
	puts("PASS: requested frame count and normal stereo ordering");
}

static void requestBounds()
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = 48000;
	Buffer output(engine, 128, 1024);
	render_process(&engine);
	require(output.chunk.size == 128 * 2 * sizeof(float), "request must fit buffer capacity");
	require(output.samples.front() == 0.0f && output.samples.back() == 0.0f,
		"underrun must produce silence");
	output.request(0);
	render_process(&engine);
	require(output.chunk.size == 128 * 2 * sizeof(float), "zero request must fall back to capacity");
	puts("PASS: oversized requests, absent suggestions and underrun silence");
}

static void recoverFromStall(int rate, size_t quantum)
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = rate;
	const size_t backlog = rate / 3;
	auto input = ramp(backlog);
	ring_push(&engine.ring, input.data(), backlog);
	Buffer output(engine, 8192, quantum);
	render_process(&engine);
	require(output.samples.front() >= static_cast<float>(backlog - quantum * 2),
		"a render stall must recover to recent audio instead of replaying the backlog");
	require(ring_avail(&engine.ring) <= quantum, "a render stall must not leave persistent delay");
	require(engine.frames_dropped > 0, "latency recovery drops must be counted");
	const auto resumed = ramp(quantum);
	ring_push(&engine.ring, resumed.data(), quantum);
	render_process(&engine);
	require(ring_avail(&engine.ring) <= quantum, "delay must remain bounded after recovery");
	puts("PASS: transient render stall recovers without persistent delay");
}

static void flushOnReconnect()
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = 48000;
	auto input = ramp(8192);
	ring_push(&engine.ring, input.data(), 8192);
	engine.flush_render = true;
	Buffer output(engine, 8192, 256);
	render_process(&engine);
	require(ring_avail(&engine.ring) == 0 && output.samples[510] == 0,
		"reconnect must discard old audio");
	puts("PASS: reconnect flush does not replay stale audio");
}

static void flushOnSuspendAndOverflow()
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = 48000;
	engine.bypass = true;
	engine.has_render = true;
	auto input = ramp(RING_FRAMES - 1);
	ring_push(&engine.ring, input.data(), RING_FRAMES - 1);
	Buffer captured(engine, 256, 0);
	captured.chunk.size = captured.data.maxsize;
	capture_process(&engine);
	require(engine.flush_render, "ring overflow must invalidate stale queued audio");
	Buffer output(engine, 8192, 256);
	render_process(&engine);
	require(ring_avail(&engine.ring) == 0 && output.samples[510] == 0,
		"overflow recovery must not replay audio older than the ring capacity");
	ring_push(&engine.ring, input.data(), 256);
	render_state_changed(&engine, PW_STREAM_STATE_STREAMING, PW_STREAM_STATE_PAUSED, nullptr);
	render_process(&engine);
	require(ring_avail(&engine.ring) == 0 && output.samples[510] == 0,
		"output suspension must invalidate queued audio");
	puts("PASS: full-ring overflow and output suspend discard stale audio");
}

static void wraparoundAndPause()
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = 48000;
	Buffer output(engine, 8192, 256);
	auto input = ramp(256);
	/* Enough cycles to wrap the ring repeatedly, interspersed with stalls. */
	for (size_t cycle = 0; cycle < 1500; ++cycle) {
		const size_t captures = cycle % 29 == 0 ? 20 : 1;
		for (size_t i = 0; i < captures; ++i)
			ring_push(&engine.ring, input.data(), 256);
		render_process(&engine);
		require(ring_avail(&engine.ring) <= 256, "repeated stalls must not accumulate latency");
		require(output.samples[510] == 255 && output.samples[511] == -255,
			"ring wraparound must preserve recent stereo audio");
	}
	/* No more capture: at most one pending cycle, then silence. */
	render_process(&engine);
	render_process(&engine);
	require(output.samples.front() == 0 && output.samples[510] == 0,
		"pausing capture must not leave a long audio tail");
	puts("PASS: repeated stalls, ring wraparound and prompt silence after pause");
}

static void legacyFrameRequest()
{
	FXPW engine{};
	engine.ch = 2;
	engine.rate = 44100;
	Buffer output(engine, 8192, 0);
	output.rate_match.size = 300;
	render_process(&engine);
	require(output.pw.size == 300, "legacy resampler demand must override capacity");
	output.rate_match.size = 0;
	spa_io_position position{};
	position.clock.duration = 256;
	position.clock.rate = {1, 48000};
	render_io_changed(&engine, SPA_IO_Position, &position, sizeof(position));
	render_process(&engine);
	require(output.pw.size == 236, "graph quantum must be converted to stream rate");
	render_io_changed(&engine, SPA_IO_Position, nullptr, 0);
	render_io_changed(&engine, SPA_IO_RateMatch, nullptr, 0);
	require(render_frame_request(&engine, &output.pw) == 0, "removed IO must not be retained");
	puts("PASS: legacy PipeWire resampler and graph quantum frame requests");
}

int main()
{
	try {
		requestedFrames();
		requestBounds();
		recoverFromStall(48000, 256);
		recoverFromStall(44100, 512);
		recoverFromStall(96000, 2048);
		flushOnReconnect();
		flushOnSuspendAndOverflow();
		wraparoundAndPause();
		legacyFrameRequest();
		return 0;
	} catch (const std::exception &error) {
		fprintf(stderr, "FAIL: %s\n", error.what());
		return 1;
	}
}
