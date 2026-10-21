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
#include <mutex>
#include <thread>

/* ------------------------------------------------------------------ */
/* Ring buffer: capture -> render (stereo interleaved floats)          */

#define RING_FRAMES 65536

struct ring {
	float buf[RING_FRAMES * 2];
	size_t rd;          /* read index, frames */
	size_t wr;          /* write index, frames */
};

static void ring_init(struct ring *r)
{
	r->rd = r->wr = 0;
}

static size_t ring_avail(const struct ring *r)
{
	return (r->wr + RING_FRAMES - r->rd) % RING_FRAMES;
}

static size_t ring_push(struct ring *r, const float *src, size_t frames)
{
	size_t free_frames = (r->rd + RING_FRAMES - 1 - r->wr) % RING_FRAMES;

	if (frames > free_frames)
		frames = free_frames;

	for (size_t f = 0; f < frames; f++) {
		r->buf[r->wr * 2 + 0] = *src++;
		r->buf[r->wr * 2 + 1] = *src++;
		r->wr = (r->wr + 1) % RING_FRAMES;
	}

	return frames;
}

static size_t ring_pop(struct ring *r, float *dst, size_t frames)
{
	size_t avail = ring_avail(r);

	if (frames > avail)
		frames = avail;

	for (size_t f = 0; f < frames; f++) {
		dst[f * 2 + 0] = r->buf[r->rd * 2 + 0];
		dst[f * 2 + 1] = r->buf[r->rd * 2 + 1];
		r->rd = (r->rd + 1) % RING_FRAMES;
	}

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
	bool ready;            /* reached ST_RUN */
	bool failed;
	char err_buf[128];

	struct pw_stream *capture;
	struct spa_hook capture_listener;
	struct pw_stream *render;
	struct spa_hook render_listener;

	/* graph */
	uint32_t sink_node_id;      /* SauceFX virtual sink */
	uint32_t out_node_id;       /* explicit --out device (0 = unset) */
	uint32_t real_sink_node_id; /* first real Audio/Sink != virtual sink */
	char out_name[96];
	char sink_name[48];
	char real_sink_name[96];
	long module_index;

	char orig_default[96]; /* default sink to restore on exit */
	int did_default;       /* switched the system default this run */
	char instance_tag[32]; /* "fxpw-<pid>", disambiguates our sink */

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
	log_info(a, "FATAL: %s", msg);
	pw_main_loop_quit(a->loop);
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
	size_t wrote = ring_push(&a->ring, src, (size_t)frames);
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

static const struct pw_stream_events render_events = {
	PW_VERSION_STREAM_EVENTS,
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

	if (a->out_node_id == 0 && a->out_name[0] != '\0' &&
	    strcmp(name, a->out_name) == 0) {
		a->out_node_id = id;
		log_info(a, "output node %u (%s)", id, name);
	}

	/* Remember the first real output device so the render stream can be
	 * pinned to it. Without this the render stream connects to "the
	 * default sink" - and as soon as routing makes the VIRTUAL sink the
	 * default, the DSP output loops back into its own capture (silent
	 * speakers, then saturating feedback = static once un-routed). */
	if (a->real_sink_node_id == 0 && media_class != NULL &&
	    strcmp(media_class, "Audio/Sink") == 0 &&
	    strcmp(name, a->sink_name) != 0) {
		a->real_sink_node_id = id;
		snprintf(a->real_sink_name, sizeof(a->real_sink_name), "%s",
			 name);
		log_info(a, "real output node %u (%s)", id, name);
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
	if (a->out_node_id != 0 && id == a->out_node_id) {
		a->out_node_id = 0;
		log_info(a, "render target node disappeared; "
			    "stream follows the current route");
	}
}

static const struct pw_registry_events registry_events = {
	PW_VERSION_REGISTRY_EVENTS,
	.global = on_global,
	.global_remove = on_global_remove,
};

/* ------------------------------------------------------------------ */
/* virtual sink lifecycle (pactl / pipewire-pulse)                     */

static void set_default_sink(FXPW *a, const char *name)
{
	char cmd[160];

	snprintf(cmd, sizeof(cmd), "pactl set-default-sink %s 2>/dev/null", name);
	if (!system(cmd))
		log_info(a, "default sink -> '%s'", name);
}

static char *get_default_sink(void)
{
	static char name[96];

	FILE *p = popen("pactl get-default-sink", "r");
	if (p == NULL)
		return NULL;
	if (fgets(name, sizeof(name), p) == NULL) {
		pclose(p);
		return NULL;
	}
	pclose(p);
	name[strcspn(name, "\r\n")] = '\0';
	return name;
}

/*
 * Our pulse module rows look like:
 *   <idx>\tmodule-null-sink\t sink_name=SauceFX sink_properties='device.description=SauceFX (fxpw-1234)'
 * The tag ties a module to its owning PID so that (a) stale-sink reaps
 * never hit a LIVE sibling instance and (b) the on-exit unload only ever
 * frees a module that still carries our own tag (pulse module indexes
 * get REUSED - a naive unload once killed a live peer's sink).
 */
static bool process_alive(long pid)
{
	if (pid <= 0)
		return false;
	if (kill((pid_t)pid, 0) != 0 && errno != EPERM)
		return false;              /* ESRCH: gone */
	if (errno == EPERM)
		return true;               /* alive, other user */

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
	return *r != 'Z';
}

/* Parse "fxpw-<pid>" out of a module argument; -1 when untagged. */
static long tag_pid_of(const char *args)
{
	const char *pat = strstr(args, "fxpw-");
	if (pat == NULL)
		return -1;
	return strtol(pat + strlen("fxpw-"), NULL, 10);
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
		if (pid == (long)getpid())
			continue;          /* this process, spawning below */
		if (process_alive(pid))
			continue;          /* a live sibling instance */
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

static void connect_streams(FXPW *a)
{
	uint8_t pod[1024];
	struct spa_pod_builder b = SPA_POD_BUILDER_INIT(pod, sizeof(pod));
	const struct spa_pod *params[1];
	struct spa_audio_info_raw info;
	spa_zero(info);
	info.format = SPA_AUDIO_FORMAT_F32;
	info.channels = 2;
	info.rate = 0;
	info.position[0] = SPA_AUDIO_CHANNEL_FL;
	info.position[1] = SPA_AUDIO_CHANNEL_FR;
	params[0] = spa_format_audio_raw_build(&b, SPA_PARAM_EnumFormat, &info);

	/* Capture: pinned to the SauceFX sink's monitor. "node.dont-move"
	 * keeps WirePlumber's default metadata ("target.node = -1" = follow
	 * the default) from overriding our target - without it WP drops the
	 * pin and the capture rides whatever the current default is. */
	struct pw_properties *pc = pw_properties_new(
		PW_KEY_MEDIA_TYPE, "Audio",
		PW_KEY_MEDIA_CATEGORY, "Capture",
		PW_KEY_STREAM_CAPTURE_SINK, "true",
		PW_KEY_TARGET_OBJECT, a->sink_name,
		"node.dont-move", "true",
		NULL);
	a->capture = pw_stream_new(a->core, "SauceFX - capture", pc);

	/* Render: pinned to a REAL output device (never the virtual sink,
	 * and never "the default" - routing moves the default onto the
	 * virtual sink, which must not drag the render stream with it).
	 * "node.dont-move" = WirePlumber may not override a defined target
	 * with its default-routing metadata. */
	const char *ren_name = a->out_name[0] != '\0' ? a->out_name
			     : a->real_sink_name[0] ? a->real_sink_name
			     : NULL;
	struct pw_properties *pr = pw_properties_new(
		PW_KEY_MEDIA_TYPE, "Audio",
		PW_KEY_MEDIA_CATEGORY, "Playback",
		"node.dont-move", "true",
		NULL);
	if (ren_name != NULL)
		pw_properties_set(pr, PW_KEY_TARGET_OBJECT, ren_name);
	a->render = pw_stream_new(a->core, "SauceFX - render", pr);

	/* NOTE on stream naming: WirePlumber's stream-properties state
	 * replays previously saved volumes by media name. The old names
	 * ("fxsound-render") carry a poisoned entry from the early
	 * feedback-loop episode (0.19); unique display names keep user
	 * volume intent working without ever matching stale entries. */

	if (a->capture == NULL || a->render == NULL) {
		set_error(a, "pw_stream_new failed");
		return;
	}

	spa_zero(a->capture_listener);
	pw_stream_add_listener(a->capture, &a->capture_listener, &capture_events, a);
	spa_zero(a->render_listener);
	pw_stream_add_listener(a->render, &a->render_listener, &render_events, a);

	if (pw_stream_connect(a->capture, PW_DIRECTION_INPUT, a->sink_node_id,
			      (enum pw_stream_flags)(PW_STREAM_FLAG_AUTOCONNECT |
						     PW_STREAM_FLAG_MAP_BUFFERS |
						     PW_STREAM_FLAG_RT_PROCESS),
			      params, 1) < 0) {
		set_error(a, "capture pw_stream_connect failed");
		return;
	}

	/* Render to the pinned real device: --out, else the original default
	 * when routing changed it, else the first real sink. PW_ID_ANY only
	 * as a last resort (unset). */
	uint32_t ren_target = a->out_node_id ? a->out_node_id
			    : a->real_sink_node_id ? a->real_sink_node_id
			    : PW_ID_ANY;
	log_info(a, "render target: %s (node %u)",
		 ren_name != NULL ? ren_name : "(unpinned default)", ren_target);
	if (pw_stream_connect(a->render, PW_DIRECTION_OUTPUT, ren_target,
			      (enum pw_stream_flags)(PW_STREAM_FLAG_AUTOCONNECT |
						     PW_STREAM_FLAG_MAP_BUFFERS |
						     PW_STREAM_FLAG_RT_PROCESS),
			      params, 1) < 0) {
		set_error(a, "render pw_stream_connect failed");
		return;
	}

	a->state = FXPW_STATE_PREFILL;
	a->clock_ms = 0.0;
	log_info(a, "capture+render connected; prefilling %d frames...",
		 PREFILL_FRAMES);
}

static void timer_tick(FXPW *a, uint64_t expirations)
{
	a->clock_ms += 100.0 * (expirations > 0 ? (double)expirations : 1.0);

	switch (a->state) {
	case FXPW_STATE_WAIT:
		if (a->sink_node_id != 0) {
			if (a->out_name[0] != '\0' && a->out_node_id == 0)
				break;   /* still hunting the --out node */
			if (a->out_name[0] == '\0' &&
			    a->real_sink_node_id == 0)
				break;   /* no real output seen yet */
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
	if (a->render) {
		pw_stream_set_active(a->render, false);
		pw_stream_destroy(a->render);
		a->render = NULL;
	}
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

	/* Zero the plain-C parts first, then construct the C++ members on
	 * top (same ordering rule the CLI used: DfxDsp must be constructed
	 * after the memset, never wiped by it). */
	a = (FXPW *)calloc(1, sizeof(FXPW));
	if (a == NULL) {
		if (err) snprintf(err, errsz, "out of memory");
		return NULL;
	}
	snprintf(a->instance_tag, sizeof(a->instance_tag), "fxpw-%d",
		 (int)getpid());
	ring_init(&a->ring);
	snprintf(a->sink_name, sizeof(a->sink_name), "%s",
		 sink_name != NULL ? sink_name : "SauceFX");
	if (out_name != NULL)
		snprintf(a->out_name, sizeof(a->out_name), "%s", out_name);
	a->bypass = bypass != 0;
	a->stats = stats != 0;
	a->power_on = true;
	a->state = FXPW_STATE_WAIT;
	a->make_default = make_default != 0;
	if (preset_path != NULL)
		a->pending_preset_path.assign(preset_path);
	if (!a->pending_preset_path.empty())
		a->pending_preset = true;

	a->dsp = new DfxDsp;

	if (spawn_null_sink(a) != 0) {
		if (err) snprintf(err, errsz,
				  "failed to create virtual sink (is pipewire-pulse running?)");
		delete a->dsp;
		free(a);
		return NULL;
	}

	if (a->make_default) {
		char *orig = get_default_sink();
		if (orig != NULL) {
			snprintf(a->orig_default, sizeof(a->orig_default), "%s", orig);
			if (strcmp(a->orig_default, a->sink_name) != 0) {
				a->did_default = 1;
				set_default_sink(a, a->sink_name);
				/* Pin the render stream to the ORIGINAL
				 * device: it must not follow the new
				 * default (which is the virtual sink). */
				if (a->out_name[0] == '\0')
					snprintf(a->out_name,
						 sizeof(a->out_name), "%s",
						 a->orig_default);
			}
		}
	}

	/* Always remember the starting default device as the restore
	 * point for route-off (and for exit). If the default currently IS
	 * our virtual sink (e.g. WirePlumber or a previous session left it
	 * routed), this still records the truth captured before any of our
	 * actions this session - the route toggle only ever touches
	 * orig_default when it performs a fresh takeover. */
	if (a->orig_default[0] == '\0') {
		char *start_default = get_default_sink();
		if (start_default != NULL)
			snprintf(a->orig_default, sizeof(a->orig_default), "%s",
				 start_default);
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
	if (h->did_default) {
		set_default_sink(h, h->orig_default);
		h->did_default = 0;
	}

	delete h->dsp;
	free(h);
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

	if (on) {
		char *cur = get_default_sink();
		if (cur == NULL)
			return -1;
		if (strcmp(cur, h->sink_name) == 0) {
			/* The default is already ours (leftover routing from
			 * an earlier session, or the user's own doing).
			 * Nothing to flip - but we ARE routed, so remember
			 * the restore claim; otherwise toggle-off would be
			 * a no-op and the system would stay on the virtual
			 * sink forever. */
			h->did_default = 1;
			return 0;
		}
		/* Fresh takeover: the current default is the restore point. */
		snprintf(h->orig_default, sizeof(h->orig_default), "%s", cur);
		h->did_default = 1;
		set_default_sink(h, h->sink_name);
		return 0;
	}

	/* OFF: restore the real device default, unconditionally when this
	 * session took it over, and additionally heal the leftover case
	 * where the default quietly stayed on our virtual sink even though
	 * this session never routed (start-of-session capture). */
	if (h->did_default) {
		set_default_sink(h, h->orig_default);
		h->did_default = 0;
	} else if (h->orig_default[0] != '\0') {
		char *cur = get_default_sink();
		if (cur != NULL && strcmp(cur, h->sink_name) == 0)
			set_default_sink(h, h->orig_default);
	}
	return 0;
}

extern "C" int fxpw_is_routed_default(FXPW *h)
{
	if (h == NULL)
		return 0;
	char *cur = get_default_sink();
	return (cur != NULL && strcmp(cur, h->sink_name) == 0) ? 1 : 0;
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
