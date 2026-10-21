/*
 * fxpw - SauceFX real-time PipeWire bridge, library form.
 *
 * Embeds the PipeWire pipeline (virtual "SauceFX" null sink -> monitor
 * capture -> DfxDsp float path -> render stream) so both the saucefxrt
 * CLI and desktop app drive the identical engine/pipeline code.
 *
 * Threading: fxpw_start() runs the PipeWire loop (and the processAudio
 * callbacks) on its own worker thread. The fxpw_* control calls are meant
 * to be made from the caller's GUI thread WITHOUT locks, exactly like the
 * Windows app (FxController mutates DfxDsp from the message thread while
 * AudioPassthru processes concurrently). Do not call fxpw_* concurrently
 * from two threads; do not call fxpw_stop twice.
 */
#ifndef FXPW_H
#define FXPW_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct FXPW FXPW;

/*
 * Bring up the virtual sink and the pw streams, then run forever until
 * fxpw_stop(). Blocks until the pipeline has actually started (or failed)
 * so callers receive the same bundled errors the CLI did. sink_name may
 * be NULL ("SauceFX"); preset_path may be NULL (no preset; engine runs in
 * its fresh registry state). Returns NULL on failure with err filled.
 */
FXPW *fxpw_start(const char *sink_name, const wchar_t *preset_path,
		 const char *out_name,
		 int make_default, int bypass, int stats, int quiet,
		 char *err, size_t errsz);

/* Clean shutdown: joins the worker, unloads the sink module, restores
 * the default sink if it was switched. */
void fxpw_stop(FXPW *h);

/* Engine control. effect indices follow DfxDsp::Effect (0..4). Value
 * scale is the UI's 0..10 like the Windows GUI (pushed *10 internally). */
int fxpw_load_preset(FXPW *h, const wchar_t *path);   /* 0 = ok */
void fxpw_set_power(FXPW *h, int on);
int fxpw_get_power(FXPW *h);
void fxpw_set_effect_value(FXPW *h, int effect, float value);
float fxpw_get_effect_value(FXPW *h, int effect);

/* EQ band accessors (bands go 0..getNumEqBands()-1). */
int fxpw_get_num_eq_bands(FXPW *h);
float fxpw_get_eq_band_freq(FXPW *h, int band);
float fxpw_get_eq_band_boostcut(FXPW *h, int band);
void fxpw_set_eq_band(FXPW *h, int band, float freq, float gain);

/* Preset display name from a .fac file (for list boxes). Returns 0 ok. */
int fxpw_preset_name(FXPW *h, const wchar_t *path, char *out, size_t outsz);

/* Write the current live engine state as <name>.fac into directory dir.
 * (The engine appends the .fac suffix itself.) Returns 0 ok. */
int fxpw_save_preset(FXPW *h, const wchar_t *name, const wchar_t *dir);

/* Route the system default output into the virtual sink (on=1, restore
 * the previous default on on=0). Returns 0 ok. */
int fxpw_route_default(FXPW *h, int on);
int fxpw_is_routed_default(FXPW *h);

/* Spectrum analyzer values from the engine (per band), like the GUI
 * visualizer on Windows. bands gets 'count' floats (linear magnitude). */
void fxpw_get_spectrum(FXPW *h, float *bands, int count);

/* Pipeline counters for status displays. */
void fxpw_get_stats(FXPW *h, long long *frames, long long *dropped,
		    long long *underruns, int *rate, int *channels,
		    int *state, int *ring_frames);

/* state codes returned via *state in fxpw_get_stats() */
#define FXPW_STATE_ERROR    0
#define FXPW_STATE_WAIT     1
#define FXPW_STATE_CONNECT  2
#define FXPW_STATE_PREFILL  3
#define FXPW_STATE_RUN      4

#ifdef __cplusplus
}
#endif

#endif /* FXPW_H */
