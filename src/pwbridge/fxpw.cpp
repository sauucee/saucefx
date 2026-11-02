/*
 * fxpw - SauceFX real-time PipeWire bridge, library form.
 *
 * Same machinery the saucefxrt CLI drives: a virtual "SauceFX" null sink
 * captures into the FxSound DfxDsp float path and renders to the real
 * device. pw objects live on this library's own worker thread; control
 * calls may be made from the caller's GUI thread exactly like the FxSound
 * app mutates DfxDsp from its message thread.
 *
 * Split out of saucefxrt.cpp into a reusable library (behavior-preserving).
 */
#include <pipewire/pipewire.h>
#include <spa/param/audio/format-utils.h>
#include <spa/param/props.h>
#include <spa/utils/result.h>

#include <DfxDsp.h>
#include <dfxpDefs.h>

#include "fxpw.h"
#include "outputs.h"

#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wchar.h>

#include <errno.h>
#include <signal.h>
#include <sys/types.h>

#include <condition_variable>
#include <atomic>
#include <memory>
#include <mutex>
#include <new>
#include <thread>
#include <unordered_map>

/* ------------------------------------------------------------------ */
/* Ring buffer: capture -> render (stereo interleaved floats)          */

#define RING_FRAMES 65536

struct ring {
	float buf[RING_FRAMES * 2];
	std::atomic<size_t> rd{0}; /* consumer owns rd; producer owns wr */
	std::atomic<size_t> wr{0};
};

static void ring_init(struct ring *r)
{
	r->rd.store(0);
	r->wr.store(0);
}

static size_t ring_avail(const struct ring *r)
{
	return (r->wr.load(std::memory_order_acquire) + RING_FRAMES -
		r->rd.load(std::memory_order_acquire)) % RING_FRAMES;
}

static size_t ring_push(struct ring *r, const float *src, size_t frames)
{
	size_t wr = r->wr.load(std::memory_order_relaxed);
	size_t free_frames = (r->rd.load(std::memory_order_acquire) +
		RING_FRAMES - 1 - wr) % RING_FRAMES;

	if (frames > free_frames)
		frames = free_frames;

	for (size_t f = 0; f < frames; f++) {
		r->buf[wr * 2 + 0] = *src++;
		r->buf[wr * 2 + 1] = *src++;
		wr = (wr + 1) % RING_FRAMES;
	}
	r->wr.store(wr, std::memory_order_release);

	return frames;
}

static size_t ring_pop(struct ring *r, float *dst, size_t frames)
{
	size_t rd = r->rd.load(std::memory_order_relaxed);
	size_t avail = (r->wr.load(std::memory_order_acquire) + RING_FRAMES - rd) % RING_FRAMES;

	if (frames > avail)
		frames = avail;

	for (size_t f = 0; f < frames; f++) {
		dst[f * 2 + 0] = r->buf[rd * 2 + 0];
		dst[f * 2 + 1] = r->buf[rd * 2 + 1];
		rd = (rd + 1) % RING_FRAMES;
	}
	r->rd.store(rd, std::memory_order_release);

	return frames;
}

/* ------------------------------------------------------------------ */

#define PREFILL_FRAMES 4096

struct FXPW {
	/* pw objects (loop thread only) */
	struct pw_main_loop *loop;
	struct pw_context *ctx;
	struct pw_core *core;
	struct pw_registry *registry;
	struct spa_hook registry_listener;
	struct spa_source *timer;

	int state;             /* FXPW_STATE_* */
	double clock_ms;       /* loop ticks for startup timeouts/statistics */
	int rc;

	std::thread worker;
	std::mutex mtx;
	std::condition_variable cv;
	std::atomic<bool> ready{false}; /* reached ST_RUN */
	std::atomic<bool> failed{false};
	char err_buf[128];

	struct pw_stream *capture;
	struct spa_hook capture_listener;
	struct pw_stream *render;
	struct spa_hook render_listener;

	/* graph */
	uint32_t sink_node_id;      /* SauceFX virtual sink */
	char sink_name[48];
	std::unordered_map<uint32_t, std::string> output_nodes;
	uint32_t render_node_id;
	bool render_failed;
	std::atomic<bool> has_render{false};
	std::atomic<bool> flush_render{false};
	std::unique_ptr<saucefx::OutputMonitor> outputs;
	std::mutex output_mutex;
	FXPWOutputState output_state{};
	bool restore_pending;
	double route_retry_ms;
	double port_retry_ms;
	double render_retry_ms;
	long module_index;

	char instance_tag[32]; /* "fxpw-<pid>-<starttime>", disambiguates our sink */

	/* negotiated capture format */
	int ch;
	int rate;

	/* dsp */
	DfxDsp *dsp;
	bool dsp_inited;
	bool bypass;
	bool stats;
	long cap_buffers;

	bool power_on;              /* GUI power switch */
	bool pending_preset;         /* push this preset on the next buffer */
	std::wstring pending_preset_path;

	struct ring ring;
	long long frames_captured;
	long long frames_dropped;
	long long underruns;
	double stats_ms;

	bool quiet;
	bool make_default;
};

static void log_info(FXPW *a, const char *fmt, ...)
{
	if (a->quiet)
		return;
	va_list ap;
	va_start(ap, fmt);
	fprintf(stderr, "fxpw: ");
	vfprintf(stderr, fmt, ap);
	fprintf(stderr, "\n");
	va_end(ap);
}

static void set_error(FXPW *a, const char *msg)
{
	if (a->err_buf[0] == '\0')
		snprintf(a->err_buf, sizeof(a->err_buf), "%s", msg);
	a->failed = true;
	a->state = FXPW_STATE_ERROR;
	log_info(a, "FATAL: %s", msg);
	if (a->loop != nullptr)
		pw_main_loop_quit(a->loop);
	a->cv.notify_all();
}

/* ------------------------------------------------------------------ */
/* DSP control - mirrors FxController::selectPreset + AudioPassthru    */

/* Push a preset into the live engine the way FxController::selectPreset
 * does after loadPreset: values, then EQ band positions/gains. Safe to
 * call before the engine is initialized (calls just no-op then). */
static int push_preset(FXPW *a, const std::wstring &path)
{
	if (a->dsp->loadPreset(path) != 0) {
		log_info(a, "loadPreset FAILED");
		return -1;
	}
	for (int e = 0; e < DfxDsp::Effect::NumEffects; e++) {
		auto v = a->dsp->getEffectValue(static_cast<DfxDsp::Effect>(e));
		a->dsp->setEffectValue(static_cast<DfxDsp::Effect>(e), v * 10);
	}
	int nb = a->dsp->getNumEqBands();
	for (int b = 0; b < nb; b++) {
		a->dsp->setEqBandFrequency(b, a->dsp->getEqBandFrequency(b));
		a->dsp->setEqBandBoostCut(b, a->dsp->getEqBandBoostCut(b));
	}
	log_info(a, "preset applied (%d eq bands)", nb);
	return 0;
}

static void dsp_setup(FXPW *a, int bps, int nch, int srate)
{
	bool first = !a->dsp_inited;

	/* First call commonly returns NOT_OKAY until fully_initialized
	 * (matches AudioPassthruPrivate.cpp:533); call every buffer like
	 * the Windows code does. */
	a->dsp->setSignalFormat(bps, nch, srate, bps);
	a->dsp_inited = true;

	if (a->pending_preset) {
		a->pending_preset = false;
		if (push_preset(a, a->pending_preset_path) != 0)
			return;
		a->dsp->powerOn(a->power_on);
		log_info(a, "dsp powered on (with preset)");
	} else if (first) {
		a->dsp->powerOn(a->power_on);
		log_info(a, "dsp powered on");
	}
}

/* ------------------------------------------------------------------ */
/* capture stream: SauceFX sink monitor -> DSP -> ring                */

static void capture_process(void *data)
{
	FXPW *a = (FXPW *)data;
	struct pw_buffer *b = pw_stream_dequeue_buffer(a->capture);

	if (b == NULL)
		return;

	struct spa_data *d = &b->buffer->datas[0];

	if (d->data == NULL || d->chunk->size == 0) {
		pw_stream_queue_buffer(a->capture, b);
		return;
	}

	const int nch = a->ch > 0 ? a->ch : 2;
	float *src = (float *)((uint8_t *)d->data + d->chunk->offset);
	long frames = (long)d->chunk->size / (long)(sizeof(float) * nch);

	if (frames > 0 && !a->bypass) {
		dsp_setup(a, 32, nch, a->rate);
		a->dsp->processAudio((short int *)src, (short int *)src,
				     (int)frames, 0);
	}

	long long cap = frames;
	size_t wrote = a->has_render.load(std::memory_order_acquire)
		? ring_push(&a->ring, src, (size_t)frames) : 0;
	a->frames_captured += (long long)wrote;
	a->frames_dropped += cap - (long long)wrote;

	if (a->stats && frames > 0 && (a->cap_buffers++ % 100) == 0) {
		double sum = 0;
		for (long i = 0; i < frames * nch; i++)
			sum += (double)src[i] * (double)src[i];
		double rms = sqrt(sum / (frames * nch));
		log_info(a, "cap rms=%.5f chunk=%ld ring=%zu", rms, frames,
			 ring_avail(&a->ring));
	}

	pw_stream_queue_buffer(a->capture, b);
}

static void capture_param_changed(void *data, uint32_t id, const struct spa_pod *param);

static const struct pw_stream_events capture_events = {
	PW_VERSION_STREAM_EVENTS,
	.param_changed = capture_param_changed,
	.process = capture_process,
};

static void capture_param_changed(void *data, uint32_t id, const struct spa_pod *param)
{
	FXPW *a = (FXPW *)data;

	if (param == NULL || id != SPA_PARAM_Format)
		return;

	struct spa_audio_info info;
	spa_zero(info);
	if (spa_format_parse(param, &info.media_type, &info.media_subtype) < 0)
		return;
	if (info.media_type != SPA_MEDIA_TYPE_audio ||
	    info.media_subtype != SPA_MEDIA_SUBTYPE_raw)
		return;
	if (spa_format_audio_raw_parse(param, &info.info.raw) < 0)
		return;

	a->ch = info.info.raw.channels;
	a->rate = info.info.raw.rate;
	log_info(a, "capture negotiated: %d ch @ %d Hz, float32", a->ch, a->rate);
}

/* ------------------------------------------------------------------ */
/* render stream: ring -> real output device                           */

static void render_process(void *data)
{
	FXPW *a = (FXPW *)data;
	/* Discard queued audio on a device change. A reconnect must never play
	 * seconds of old audio accumulated while a headset was disconnected. */
	if (a->flush_render.exchange(false, std::memory_order_acq_rel))
		a->ring.rd.store(a->ring.wr.load(std::memory_order_acquire),
				std::memory_order_release);
	struct pw_buffer *b = pw_stream_dequeue_buffer(a->render);

	if (b == NULL)
		return;

	struct spa_data *d = &b->buffer->datas[0];

	if (d->data == NULL) {
		d->chunk->size = 0;
		pw_stream_queue_buffer(a->render, b);
		return;
	}

	const int nch = a->ch > 0 ? a->ch : 2;
	long frames = (long)d->maxsize / (long)(sizeof(float) * nch);

	if (frames <= 0) {
		d->chunk->size = 0;
		pw_stream_queue_buffer(a->render, b);
		return;
	}

	size_t size_bytes = (size_t)frames * sizeof(float) * nch;
	size_t got = ring_pop(&a->ring, (float *)d->data, (size_t)frames);

	if ((long)got < frames) {
		a->underruns++;
		memset((uint8_t *)d->data + got * sizeof(float) * nch, 0,
		       size_bytes - got * sizeof(float) * nch);
	}

	d->chunk->offset = 0;
	d->chunk->size = (uint32_t)size_bytes;
	d->chunk->stride = sizeof(float) * nch;

	pw_stream_queue_buffer(a->render, b);
}

static void render_state_changed(void *data, enum pw_stream_state,
				 enum pw_stream_state state, const char *error)
{
	auto *a = static_cast<FXPW *>(data);
	if (state == PW_STREAM_STATE_ERROR) {
		a->render_failed = true;
		log_info(a, "output stream unavailable: %s", error != nullptr ? error : "unknown error");
	}
}

static const struct pw_stream_events render_events = {
	PW_VERSION_STREAM_EVENTS,
	.state_changed = render_state_changed,
	.process = render_process,
};

/* ------------------------------------------------------------------ */
/* registry: find our virtual sink (and optional named output device)  */

static void on_global(void *data, uint32_t id, uint32_t permissions,
		      const char *type, uint32_t version,
		      const struct spa_dict *props)
{
	FXPW *a = (FXPW *)data;

	if (props == NULL || strcmp(type, PW_TYPE_INTERFACE_Node) != 0)
		return;

	const char *name = spa_dict_lookup(props, PW_KEY_NODE_NAME);
	if (name == NULL)
		return;
	const char *media_class = spa_dict_lookup(props, PW_KEY_MEDIA_CLASS);

	if (a->sink_node_id == 0 && strcmp(name, a->sink_name) == 0) {
		a->sink_node_id = id;
		log_info(a, "virtual sink node %u (%s)", id, name);
	}

	if (media_class != NULL &&
	    strcmp(media_class, "Audio/Sink") == 0 &&
	    strcmp(name, a->sink_name) != 0) {
		a->output_nodes[id] = name;
	}
}

static void on_global_remove(void *data, uint32_t id)
{
	FXPW *a = (FXPW *)data;

	/* If our virtual sink dies out from under us (killed module, daemon
	 * restart), bail loudly: silently riding the default-node capture
	 * would turn into a self-feedback loop through the ALSA monitor. */
	if (a->sink_node_id != 0 && id == a->sink_node_id &&
	    a->state != FXPW_STATE_ERROR) {
		a->sink_node_id = 0;
		set_error(a, "virtual sink disappeared (module killed?)");
		return;
	}
	a->output_nodes.erase(id);
}

static const struct pw_registry_events registry_events = {
	PW_VERSION_REGISTRY_EVENTS,
	.global = on_global,
	.global_remove = on_global_remove,
};

/* ------------------------------------------------------------------ */
/* virtual sink lifecycle (pactl / pipewire-pulse)                     */

/*
 * Our pulse module rows look like:
 *   <idx>\tmodule-null-sink\t sink_name=SauceFX sink_properties='device.description=SauceFX (fxpw-1234)'
 * The tag ties a module to its owning PID so that (a) stale-sink reaps
 * never hit a LIVE sibling instance and (b) the on-exit unload only ever
 * frees a module that still carries our own tag (pulse module indexes
 * get REUSED - a naive unload once killed a live peer's sink).
 */
/* Field 22 of /proc/<pid>/stat. Unlike the PID, it survives PID reuse,
 * including Flatpak's PID namespace where every launch becomes PID 2. */
static unsigned long long process_start_time(long pid)
{
	if (pid <= 0)
		return 0;
	char path[64];
	char buf[1024];
	snprintf(path, sizeof(path), "/proc/%ld/stat", pid);
	FILE *f = fopen(path, "r");
	if (f == NULL)
		return 0;
	size_t n = fread(buf, 1, sizeof(buf) - 1, f);
	fclose(f);
	buf[n] = '\0';
	char *r = strrchr(buf, ')');
	if (r == NULL)
		return 0;
	r++;
	while (*r == ' ')
		r++;
	if (*r == '\0')
		return 0;
	r++; /* skip state; fields 4..22 follow */
	unsigned long long value = 0;
	for (int i = 0; i < 19; ++i) {
		while (*r == ' ')
			r++;
		char *end = NULL;
		value = strtoull(r, &end, 10);
		if (end == r)
			return 0;
		r = end;
	}
	return value;
}

static bool process_alive(long pid, unsigned long long start_time)
{
	if (pid <= 0)
		return false;
	if (kill((pid_t)pid, 0) != 0 && errno != EPERM)
		return false;              /* ESRCH: gone */

	/* kill() also succeeds for unreaped zombies; /proc/<pid>/stat says
	 * state 'Z' for those. */
	char path[64];
	char buf[256];
	snprintf(path, sizeof(path), "/proc/%ld/stat", pid);
	FILE *f = fopen(path, "r");
	if (f == NULL)
		return false;
	size_t n = fread(buf, 1, sizeof(buf) - 1, f);
	fclose(f);
	buf[n] = '\0';
	const char *r = strrchr(buf, ')');
	if (r == NULL)
		return true;
	r++;
	while (*r == ' ')
		r++;
	if (*r == 'Z')
		return false;
	if (start_time != 0) {
		unsigned long long current = process_start_time(pid);
		return current != 0 && current == start_time;
	}
	return true;
}

/* Parse "fxpw-<pid>[-<starttime>]" out of a module argument. */
static long tag_pid_of(const char *args)
{
	const char *pat = strstr(args, "fxpw-");
	if (pat == NULL)
		return -1;
	pat += strlen("fxpw-");
	char *end = NULL;
	long pid = strtol(pat, &end, 10);
	return end == pat ? -1 : pid;
}

static unsigned long long tag_start_of(const char *args)
{
	const char *pat = strstr(args, "fxpw-");
	if (pat == NULL)
		return 0;
	pat += strlen("fxpw-");
	char *end = NULL;
	(void)strtol(pat, &end, 10);
	if (end == pat || *end != '-')
		return 0;
	++end;
	char *start_end = NULL;
	unsigned long long start = strtoull(end, &start_end, 10);
	return start_end == end ? 0 : start;
}

static long unload_stale_sinks(const char *sink_name, const char *our_tag_unused)
{
	(void)our_tag_unused;
	FILE *p = popen("pactl list short modules", "r");
	if (p == NULL)
		return 0;
	long reaped = 0;
	char line[768];
	while (fgets(line, sizeof(line), p)) {
		if (strstr(line, "sink_name=") == NULL ||
		    strstr(line, "module-null-sink") == NULL)
			continue;
		if (strstr(line, sink_name) == NULL)
			continue;
		long idx = strtol(line, NULL, 10);
		if (idx <= 0)
			continue;
		if (strstr(line, "fxpw-") == NULL) {
			/* legacy, pre-tagging leftover */
			char cmd[96];
			snprintf(cmd, sizeof(cmd), "pactl unload-module %ld", idx);
			if (system(cmd) == 0)
				reaped++;
			continue;
		}
		long pid = tag_pid_of(line);
		unsigned long long started = tag_start_of(line);
		if (started != 0 && process_alive(pid, started))
			continue;          /* a live sibling instance */
		if (started == 0 && pid == (long)getpid()) {
			/* Pre-starttime tag in a PID namespace: every launch
			 * reuses this PID, so the module is from a previous
			 * incarnation even though /proc now shows it alive. */
		} else if (started == 0 && process_alive(pid, 0)) {
			continue;          /* a live sibling instance */
		}
		char cmd[96];
		snprintf(cmd, sizeof(cmd), "pactl unload-module %ld", idx);
		if (system(cmd) == 0)
			reaped++;
	}
	pclose(p);
	return reaped;
}

static int spawn_null_sink(FXPW *a)
{
	unload_stale_sinks(a->sink_name, a->instance_tag);

	char cmd[512];
	snprintf(cmd, sizeof(cmd),
		 "pactl load-module module-null-sink sink_name=%s "
		 "sink_properties='device.description=SauceFX (%s)' "
		 "2>/dev/null",
		 a->sink_name, a->instance_tag);

	FILE *p = popen(cmd, "r");
	if (p == NULL)
		return -1;

	char line[128] = "";
	bool got = fgets(line, sizeof(line), p) != NULL;
	pclose(p);
	if (!got)
		return -1;

	char *end = NULL;
	long idx = strtol(line, &end, 10);
	if (line == end || idx <= 0)
		return -1;

	a->module_index = idx;
	log_info(a, "created virtual sink '%s' tagged '%s' (pulse module %ld)",
		 a->sink_name, a->instance_tag, idx);
	return 0;
}

/* ------------------------------------------------------------------ */
/* state machine (runs on the pw loop via timer)                       */

static const spa_pod *stream_format(spa_pod_builder &builder)
{
	struct spa_audio_info_raw info{};
	info.format = SPA_AUDIO_FORMAT_F32;
	info.channels = 2;
	info.rate = 0;
	info.position[0] = SPA_AUDIO_CHANNEL_FL;
	info.position[1] = SPA_AUDIO_CHANNEL_FR;
	return spa_format_audio_raw_build(&builder, SPA_PARAM_EnumFormat, &info);
}

static void disconnect_render(FXPW *a)
{
	a->has_render.store(false, std::memory_order_release);
	if (a->render != nullptr) {
		spa_hook_remove(&a->render_listener);
		pw_stream_set_active(a->render, false);
		pw_stream_destroy(a->render);
		a->render = nullptr;
	}
	a->render_node_id = 0;
}

static void connect_render(FXPW *a, const std::string &name, uint32_t id)
{
	disconnect_render(a);
	a->render_failed = false;
	a->render_retry_ms = a->clock_ms + 1000.0;
	/* The default is often our own virtual sink. Neither WirePlumber's
	 * default changes nor unplugging a headset may send DSP output there. */
	auto *props = pw_properties_new(
		PW_KEY_MEDIA_TYPE, "Audio",
		PW_KEY_MEDIA_CATEGORY, "Playback",
		PW_KEY_NODE_NAME, "saucefx.render",
		PW_KEY_TARGET_OBJECT, name.c_str(),
		"node.dont-move", "true",
		"node.dont-fallback", "true",
		NULL);
	a->render = pw_stream_new(a->core, "SauceFX - render", props);
	if (a->render == nullptr)
		return;
	spa_zero(a->render_listener);
	pw_stream_add_listener(a->render, &a->render_listener, &render_events, a);
	uint8_t pod[1024];
	spa_pod_builder builder = SPA_POD_BUILDER_INIT(pod, sizeof(pod));
	const spa_pod *params[] = {stream_format(builder)};
	a->flush_render.store(true, std::memory_order_release);
	if (pw_stream_connect(a->render, PW_DIRECTION_OUTPUT, id,
		static_cast<pw_stream_flags>(PW_STREAM_FLAG_AUTOCONNECT |
			PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_RT_PROCESS),
		params, 1) < 0) {
		disconnect_render(a);
		return;
	}
	a->render_node_id = id;
	a->has_render.store(true, std::memory_order_release);
	log_info(a, "render target: %s (node %u)", name.c_str(), id);
}

static void update_output(FXPW *a)
{
	if (a->capture == nullptr)
		return;
	auto snapshot = a->outputs->snapshot();
	std::lock_guard<std::mutex> lock(a->output_mutex);
	auto &state = a->output_state;
	const auto *target = saucefx::chooseOutput(snapshot, state.preferred_name,
						  state.preferred_port);
	uint32_t id = 0;
	if (target != nullptr)
		for (const auto &node : a->output_nodes)
			if (node.second == target->name) {
				id = node.first;
				break;
			}
	if (id == 0) {
		if (a->render != nullptr)
			log_info(a, "waiting for an available audio output");
		disconnect_render(a);
		state.active_name[0] = state.active_port[0] = '\0';
	} else {
		if (id != a->render_node_id || a->render_failed) {
			if (!a->render_failed || a->clock_ms >= a->render_retry_ms)
				connect_render(a, target->name, id);
		}
		if (a->render_node_id != 0) {
			snprintf(state.active_name, sizeof(state.active_name), "%s", target->name.c_str());
			snprintf(state.active_port, sizeof(state.active_port), "%s", target->activePort.c_str());
		}
		/* A remembered jack can return on the same ALSA sink, or after
		 * a Bluetooth profile switch recreates its sink. */
		std::string port;
		if (target->name == state.preferred_name &&
		    state.preferred_port[0] != '\0' && target->available(state.preferred_port))
			port = state.preferred_port;
		else if (!target->ports.empty() && !target->available(target->activePort)) {
			const saucefx::OutputPort *best = nullptr;
			for (const auto &p : target->ports)
				if (p.available && (best == nullptr || p.priority > best->priority))
					best = &p;
			if (best != nullptr)
				port = best->name;
		}
		if (!port.empty() && port != target->activePort && a->clock_ms >= a->port_retry_ms) {
			a->outputs->requestPort(target->name, port);
			a->port_retry_ms = a->clock_ms + 1000.0;
		}
	}
	if (snapshot.connected && a->clock_ms >= a->route_retry_ms) {
		if (state.routed && snapshot.defaultSink != a->sink_name) {
			/* An OS headset/default change updates automaticSink. Keep
			 * system streams going through the processor after retargeting. */
			a->outputs->requestDefault(a->sink_name);
			a->route_retry_ms = a->clock_ms + 1000.0;
		} else if (!state.routed && a->restore_pending && target != nullptr) {
			a->outputs->requestDefault(target->name);
			a->route_retry_ms = a->clock_ms + 1000.0;
			if (snapshot.defaultSink != a->sink_name)
				a->restore_pending = false;
		}
	}
}

static void connect_streams(FXPW *a)
{
	uint8_t pod[1024];
	spa_pod_builder builder = SPA_POD_BUILDER_INIT(pod, sizeof(pod));
	const spa_pod *params[] = {stream_format(builder)};

	/* Capture: pinned to the SauceFX sink's monitor. "node.dont-move"
	 * keeps WirePlumber's default metadata ("target.node = -1" = follow
	 * the default) from overriding our target - without it WP drops the
	 * pin and the capture rides whatever the current default is. */
	struct pw_properties *pc = pw_properties_new(
		PW_KEY_MEDIA_TYPE, "Audio",
		PW_KEY_MEDIA_CATEGORY, "Capture",
		PW_KEY_NODE_NAME, "saucefx.capture",
		PW_KEY_STREAM_CAPTURE_SINK, "true",
		PW_KEY_TARGET_OBJECT, a->sink_name,
		"node.dont-move", "true",
		"node.dont-fallback", "true",
		NULL);
	a->capture = pw_stream_new(a->core, "SauceFX - capture", pc);

	if (a->capture == NULL) {
		set_error(a, "pw_stream_new failed");
		return;
	}

	spa_zero(a->capture_listener);
	pw_stream_add_listener(a->capture, &a->capture_listener, &capture_events, a);

	if (pw_stream_connect(a->capture, PW_DIRECTION_INPUT, a->sink_node_id,
			      (enum pw_stream_flags)(PW_STREAM_FLAG_AUTOCONNECT |
						     PW_STREAM_FLAG_MAP_BUFFERS |
						     PW_STREAM_FLAG_RT_PROCESS),
			      params, 1) < 0) {
		set_error(a, "capture pw_stream_connect failed");
		return;
	}

	a->state = FXPW_STATE_PREFILL;
	a->clock_ms = 0.0;
	log_info(a, "capture connected; prefilling %d frames...",
		 PREFILL_FRAMES);
}

static void timer_tick(FXPW *a, uint64_t expirations)
{
	a->clock_ms += 100.0 * (expirations > 0 ? (double)expirations : 1.0);

	switch (a->state) {
	case FXPW_STATE_WAIT:
		if (a->sink_node_id != 0) {
			a->state = FXPW_STATE_CONNECT;
			connect_streams(a);
		} else if (a->clock_ms > 5000.0) {
			set_error(a, "virtual sink node never appeared");
		}
		break;

	case FXPW_STATE_PREFILL:
		if (ring_avail(&a->ring) >= PREFILL_FRAMES || a->clock_ms > 2000.0) {
			a->state = FXPW_STATE_RUN;
			a->stats_ms = a->clock_ms;
			{
				std::lock_guard<std::mutex> lk(a->mtx);
				a->ready = true;
			}
			a->cv.notify_all();
			log_info(a, "started (prefilled %zu frames)",
				 ring_avail(&a->ring));
			log_info(a,
				 "ready. Play audio into sink '%s' to hear it "
				 "DSP-processed.", a->sink_name);
		}
		break;

	case FXPW_STATE_RUN:
		if (a->clock_ms - a->stats_ms >= 5000.0) {
			a->stats_ms = a->clock_ms;
			log_info(a, "processed %.1f s of audio (dropped %lld, "
				 "underruns %lld, ring %zu fr)",
				 a->frames_captured /
				 (double)(a->rate > 0 ? a->rate : 48000),
				 a->frames_dropped, a->underruns,
				 ring_avail(&a->ring));
		}
		break;
	}
	if (a->state != FXPW_STATE_ERROR)
		update_output(a);
}

/* ------------------------------------------------------------------ */
/* worker thread: owns every pw object end to end                      */

	/* ------------------------------------------------------------------ */

static void bridge_thread_main(FXPW *a)
{
	pw_init(NULL, NULL);

	a->loop = pw_main_loop_new(NULL);
	a->ctx = pw_context_new(pw_main_loop_get_loop(a->loop), NULL, 0);
	if (a->ctx == NULL) {
		set_error(a, "pw_context_new failed");
		pw_deinit();
		return;
	}

	a->core = pw_context_connect(a->ctx, NULL, 0);
	if (a->core == NULL) {
		set_error(a, "cannot connect to PipeWire core");
		goto out;
	}

	a->registry = pw_core_get_registry(a->core, PW_VERSION_REGISTRY, 0);
	spa_zero(a->registry_listener);
	pw_registry_add_listener(a->registry, &a->registry_listener, &registry_events, a);

	/* registry global events only dispatch inside the loop, so the whole
	 * bring-up runs as a state machine on this timer (100 ms ticks). */
	a->timer = pw_loop_add_timer(pw_main_loop_get_loop(a->loop),
				     [](void *data, uint64_t expirations) {
					     timer_tick((FXPW *)data, expirations);
				     }, a);
	struct timespec iv;
	iv.tv_sec = 0;
	iv.tv_nsec = 100 * 1000 * 1000;
	pw_loop_update_timer(pw_main_loop_get_loop(a->loop), a->timer, &iv, &iv, false);

	log_info(a, "waiting for the '%s' node...", a->sink_name);

	pw_main_loop_run(a->loop);

	if (!a->failed && !a->ready)
		set_error(a, "pipeline stopped before running");

out:
	disconnect_render(a);
	if (a->capture) {
		pw_stream_set_active(a->capture, false);
		pw_stream_destroy(a->capture);
		a->capture = NULL;
	}
	if (a->registry) {
		spa_hook_remove(&a->registry_listener);
		pw_proxy_destroy((struct pw_proxy *)a->registry);
		a->registry = NULL;
	}
	if (a->core) {
		pw_core_disconnect(a->core);
		a->core = NULL;
	}
	if (a->ctx) {
		pw_context_destroy(a->ctx);
		a->ctx = NULL;
	}
	if (a->loop) {
		pw_main_loop_destroy(a->loop);
		a->loop = NULL;
	}

	pw_deinit();

	{
		std::lock_guard<std::mutex> lk(a->mtx);
		a->ready = true;   /* release a possibly-waiting start() */
	}
	a->cv.notify_all();
}

extern "C" FXPW *fxpw_start(const char *sink_name, const wchar_t *preset_path,
			    const char *out_name, int make_default,
			    int bypass, int stats, int quiet,
			    char *err, size_t errsz)
{
	FXPW *a;

	/* Value-initialize the C fields and construct the C++ synchronization,
	 * strings and device monitor normally. calloc cannot construct them. */
	a = new (std::nothrow) FXPW{};
	if (a == NULL) {
		if (err) snprintf(err, errsz, "out of memory");
		return NULL;
	}
	snprintf(a->instance_tag, sizeof(a->instance_tag),
		 "fxpw-%d-%llu", (int)getpid(),
		 process_start_time((long)getpid()));
	ring_init(&a->ring);
	snprintf(a->sink_name, sizeof(a->sink_name), "%s",
		 sink_name != NULL ? sink_name : "SauceFX");
	if (out_name != NULL)
		snprintf(a->output_state.preferred_name, sizeof(a->output_state.preferred_name), "%s", out_name);
	a->bypass = bypass != 0;
	a->stats = stats != 0;
	a->quiet = quiet != 0;
	a->power_on = true;
	a->state = FXPW_STATE_WAIT;
	a->make_default = make_default != 0;
	if (preset_path != NULL)
		a->pending_preset_path.assign(preset_path);
	if (!a->pending_preset_path.empty())
		a->pending_preset = true;

	a->dsp = new DfxDsp;
	a->outputs = std::make_unique<saucefx::OutputMonitor>(a->sink_name);
	std::string outputError;
	if (!a->outputs->start(outputError)) {
		if (err && errsz > 0)
			snprintf(err, errsz, "%s", outputError.c_str());
		delete a->dsp;
		delete a;
		return nullptr;
	}
	a->output_state.routed = a->outputs->snapshot().defaultSink == a->sink_name;

	if (spawn_null_sink(a) != 0) {
		if (err) snprintf(err, errsz,
				  "failed to create virtual sink (is pipewire-pulse running?)");
		delete a->dsp;
		delete a;
		return NULL;
	}

	a->worker = std::thread(bridge_thread_main, a);

	/* Wait for the pipeline to reach the running state (or fail). */
	{
		std::unique_lock<std::mutex> lk(a->mtx);
		bool ok = a->cv.wait_for(lk, std::chrono::seconds(15), [&] {
			return a->ready || a->failed;
		});
		if (!ok) {
			set_error(a, "timed out starting the pipeline");
		}
	}

	if (a->failed) {
		if (err && errsz > 0)
			snprintf(err, errsz, "%s",
				 a->err_buf[0] != '\0' ? a->err_buf : "startup failed");
		fxpw_stop(a);
		return NULL;
	}
	if (a->make_default && fxpw_route_default(a, 1) != 0) {
		if (err && errsz > 0)
			snprintf(err, errsz, "cannot route system audio to SauceFX");
		fxpw_stop(a);
		return nullptr;
	}

	return a;
}

extern "C" void fxpw_stop(FXPW *h)
{
	if (h == NULL)
		return;

	if (h->loop != NULL)
		pw_main_loop_quit(h->loop);
	if (h->worker.joinable())
		h->worker.join();
	/* Restore a still-connected device, not a stale Bluetooth sink name. */
	auto snapshot = h->outputs->snapshot();
	if (h->output_state.routed && snapshot.defaultSink == h->sink_name)
		if (const auto *target = saucefx::chooseOutput(snapshot,
			h->output_state.preferred_name, h->output_state.preferred_port))
			h->outputs->setDefault(target->name);

	if (h->module_index > 0) {
		/* Only unload if the module row still carries OUR tag:
		 * pulse module indexes are reused, so a blind unload can
		 * nuke a live sibling's sink. */
		bool ours = false;
		char cmd[128];
		snprintf(cmd, sizeof(cmd), "pactl list short modules");
		FILE *p = popen(cmd, "r");
		if (p != NULL) {
			char line[1024];
			while (fgets(line, sizeof(line), p)) {
				if (strtol(line, NULL, 10) == h->module_index &&
				    strstr(line, "module-null-sink") != NULL &&
				    h->instance_tag[0] != '\0' &&
				    strstr(line, h->instance_tag) != NULL) {
					ours = true;
					break;
				}
			}
			pclose(p);
		}
		if (ours) {
			snprintf(cmd, sizeof(cmd),
				 "pactl unload-module %ld", h->module_index);
			(void)!system(cmd);
			log_info(h, "virtual sink unloaded");
		} else {
			log_info(h, "sink module %ld no longer ours; not unloading",
				 h->module_index);
		}
	}
	delete h->dsp;
	delete h;
}

extern "C" int fxpw_load_preset(FXPW *h, const wchar_t *path)
{
	if (h == NULL || path == NULL)
		return -1;

	h->pending_preset_path.assign(path);

	/* The engine is already running in the normal GUI case. Applying
	 * the preset now is enough; leaving pending_preset set would make
	 * dsp_setup() push the .fac a second time on the next buffer and
	 * overwrite restored session values. Only queue for the first
	 * buffer when the DSP has not been initialized yet. */
	if (h->dsp_inited) {
		h->pending_preset = false;
		int rc = push_preset(h, h->pending_preset_path);
		if (rc != 0)
			h->pending_preset = true;
		return rc;
	}

	h->pending_preset = true;

	/* Try immediately; if the engine is not initialized yet the call
	 * no-ops engine-side and the pending push fires on the next buffer
	 * (dsp_setup) exactly like a startup preset would. */
	return push_preset(h, h->pending_preset_path);
}

extern "C" void fxpw_set_power(FXPW *h, int on)
{
	if (h == NULL)
		return;
	h->power_on = on != 0;
	h->dsp->powerOn(h->power_on);
}

extern "C" int fxpw_get_power(FXPW *h)
{
	return h != NULL && h->power_on;
}

extern "C" void fxpw_set_effect_value(FXPW *h, int effect, float value)
{
	if (h == NULL || effect < 0 || effect >= (int)DfxDsp::Effect::NumEffects)
		return;
	if (value < 0.0f)
		value = 0.0f;
	if (value > 10.0f)
		value = 10.0f;
	h->dsp->setEffectValue(static_cast<DfxDsp::Effect>(effect), value);
}

extern "C" float fxpw_get_effect_value(FXPW *h, int effect)
{
	if (h == NULL || effect < 0 || effect >= (int)DfxDsp::Effect::NumEffects)
		return 0.0f;
	float v = h->dsp->getEffectValue(static_cast<DfxDsp::Effect>(effect));
	return v * 10.0f;
}

extern "C" int fxpw_get_num_eq_bands(FXPW *h)
{
	return h != NULL ? h->dsp->getNumEqBands() : 0;
}

extern "C" float fxpw_get_eq_band_freq(FXPW *h, int band)
{
	return h != NULL ? h->dsp->getEqBandFrequency(band) : 0.0f;
}

extern "C" float fxpw_get_eq_band_boostcut(FXPW *h, int band)
{
	return h != NULL ? h->dsp->getEqBandBoostCut(band) : 0.0f;
}

extern "C" void fxpw_set_eq_band(FXPW *h, int band, float freq, float gain)
{
	if (h == NULL)
		return;
	h->dsp->setEqBandFrequency(band, freq);
	h->dsp->setEqBandBoostCut(band, gain);
}

extern "C" int fxpw_preset_name(FXPW *h, const wchar_t *path, char *out, size_t outsz)
{
	if (out == NULL || outsz == 0 || path == NULL)
		return -1;

	if (h != NULL) {
		DfxPreset info = h->dsp->getPresetInfo(path);
		if (!info.name.empty()) {
			size_t n = wcstombs(out, info.name.c_str(), outsz - 1);
			if (n != (size_t)-1) {
				out[n] = '\0';
				return 0;
			}
		}
	}

	/* Fallback: file stem. */
	const wchar_t *slash = wcsrchr(path, L'/');
	const wchar_t *stem = slash != NULL ? slash + 1 : path;
	size_t n = wcstombs(out, stem, outsz - 1);
	if (n == (size_t)-1)
		return -1;
	out[n] = '\0';
	char *dot = strrchr(out, '.');
	if (dot != NULL && strcmp(dot, ".fac") == 0)
		*dot = '\0';
	return 0;
}

extern "C" int fxpw_save_preset(FXPW *h, const wchar_t *name, const wchar_t *dir)
{
	if (h == NULL || name == NULL || dir == NULL || name[0] == L'\0')
		return -1;

	std::wstring preset_name(name);
	std::wstring preset_dir(dir);
	return h->dsp->savePreset(preset_name, preset_dir) == 0 ? 0 : -1;
}

extern "C" int fxpw_route_default(FXPW *h, int on)
{
	if (h == NULL)
		return -1;
	std::lock_guard<std::mutex> lock(h->output_mutex);
	auto snapshot = h->outputs->snapshot();
	if (on != 0) {
		if (!h->outputs->setDefault(h->sink_name))
			return -1;
		h->output_state.routed = 1;
		h->restore_pending = false;
	} else {
		const auto *target = saucefx::chooseOutput(snapshot,
			h->output_state.preferred_name, h->output_state.preferred_port);
		if (target != nullptr && !h->outputs->setDefault(target->name))
			return -1;
		h->output_state.routed = 0;
		h->restore_pending = target == nullptr;
	}
	return 0;
}

extern "C" int fxpw_is_routed_default(FXPW *h)
{
	if (h == NULL)
		return 0;
	std::lock_guard<std::mutex> lock(h->output_mutex);
	return h->output_state.routed;
}

extern "C" size_t fxpw_get_outputs(FXPW *h, FXPWOutput *outputs, size_t capacity)
{
	if (h == nullptr)
		return 0;
	auto snapshot = h->outputs->snapshot();
	size_t count = 0;
	for (const auto &device : snapshot.devices) {
		auto add = [&](const saucefx::OutputPort *port) {
			if (outputs != nullptr && count < capacity) {
				auto &entry = outputs[count];
				entry = {};
				snprintf(entry.name, sizeof(entry.name), "%s", device.name.c_str());
				snprintf(entry.description, sizeof(entry.description), "%s", device.description.c_str());
				entry.available = snapshot.connected && (port == nullptr || port->available);
				if (port != nullptr) {
					snprintf(entry.port, sizeof(entry.port), "%s", port->name.c_str());
					snprintf(entry.port_description, sizeof(entry.port_description), "%s", port->description.c_str());
				}
			}
			++count;
		};
		if (device.ports.empty())
			add(nullptr);
		else
			for (const auto &port : device.ports)
				add(&port);
	}
	return count;
}

extern "C" void fxpw_get_output_state(FXPW *h, FXPWOutputState *state)
{
	if (state == nullptr)
		return;
	*state = {};
	if (h == nullptr)
		return;
	std::lock_guard<std::mutex> lock(h->output_mutex);
	*state = h->output_state;
}

extern "C" int fxpw_set_output(FXPW *h, const char *name, const char *port)
{
	if (h == nullptr)
		return -1;
	std::string wanted = name != nullptr ? name : "";
	std::string wantedPort = port != nullptr ? port : "";
	if (wanted == h->sink_name || wanted == "SauceFX" || wanted == "auto_null" ||
	    wanted.find_first_of("\r\n") != std::string::npos ||
	    wantedPort.find_first_of("\r\n") != std::string::npos ||
	    wanted.size() >= sizeof(h->output_state.preferred_name) ||
	    wantedPort.size() >= sizeof(h->output_state.preferred_port))
		return -1;
	if (wanted.empty())
		wantedPort.clear();
	std::lock_guard<std::mutex> lock(h->output_mutex);
	auto snapshot = h->outputs->snapshot();
	for (const auto &device : snapshot.devices)
		if (device.name == wanted && !wantedPort.empty() &&
		    device.available(wantedPort) && device.activePort != wantedPort)
			if (!h->outputs->setPort(wanted, wantedPort))
				return -1;
	snprintf(h->output_state.preferred_name, sizeof(h->output_state.preferred_name), "%s", wanted.c_str());
	snprintf(h->output_state.preferred_port, sizeof(h->output_state.preferred_port), "%s", wantedPort.c_str());
	return 0;
}

extern "C" void fxpw_get_spectrum(FXPW *h, float *bands, int count)
{
	if (h == NULL || bands == NULL || count <= 0)
		return;

	/* The engine's analyzer requires the array size to match its
	 * configured band count exactly (spectrumGetBandValues returns
	 * NOT_OKAY otherwise), so stage internally with the real size. */
	float stage[DFXP_SPECTRUM_NUM_BANDS];
	h->dsp->getSpectrumBandValues(stage, DFXP_SPECTRUM_NUM_BANDS);

	int n = count < DFXP_SPECTRUM_NUM_BANDS ? count : DFXP_SPECTRUM_NUM_BANDS;
	for (int i = 0; i < n; i++)
		bands[i] = stage[i];
	for (int i = n; i < count; i++)
		bands[i] = 0.0f;
}

extern "C" void fxpw_get_stats(FXPW *h, long long *frames, long long *dropped,
			       long long *underruns, int *rate, int *channels,
			       int *state, int *ring_frames)
{
	if (h == NULL)
		return;
	if (frames) *frames = h->frames_captured;
	if (dropped) *dropped = h->frames_dropped;
	if (underruns) *underruns = h->underruns;
	if (rate) *rate = h->rate;
	if (channels) *channels = h->ch;
	if (state) *state = h->state;
	if (ring_frames) *ring_frames = (int)ring_avail(&h->ring);
}
