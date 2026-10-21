/*
 * saucefxrt - CLI wrapper around the fxpw real-time bridge.
 *
 * The pipeline itself lives in fxpw.cpp (shared with the GUI front-end);
 * this tool is for the terminal: play audio into the "SauceFX" virtual
 * sink and it comes out of your speakers DSP-processed. Ctrl-C stops.
 */
#include "fxpw.h"

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wchar.h>

#include <pthread.h>

static void usage(void)
{
	fprintf(stderr,
		"usage: saucefxrt [options]\n"
		"  --preset FILE   load a .fac preset at startup\n"
		"  --out NAME      render to this sink's node.name (default: system default)\n"
		"  --sink-name N   virtual sink name (default: SauceFX)\n"
		"  --default       make the virtual sink the system default\n"
		"                  (previous default restored on exit)\n"
		"  --bypass        route audio through the engine with effects off\n"
		"  --stats         per-buffer capture level/ring stats\n"
		"  --quiet         less logging\n");
}

int main(int argc, char **argv)
{
	const char *sink_name = NULL;
	const char *out_name = NULL;
	wchar_t preset_path[1024];
	int have_preset = 0;
	int make_default = 0, bypass = 0, stats = 0, quiet = 0;
	char err[192] = "";

	for (int i = 1; i < argc; i++) {
		if (!strcmp(argv[i], "--preset") && i + 1 < argc) {
			size_t n = mbstowcs(preset_path, argv[++i],
					    sizeof(preset_path) / sizeof(preset_path[0]) - 1);
			have_preset = n != (size_t)-1;
		} else if (!strcmp(argv[i], "--out") && i + 1 < argc) {
			out_name = argv[++i];
		} else if (!strcmp(argv[i], "--sink-name") && i + 1 < argc) {
			sink_name = argv[++i];
		} else if (!strcmp(argv[i], "--default")) {
			make_default = 1;
		} else if (!strcmp(argv[i], "--bypass")) {
			bypass = 1;
		} else if (!strcmp(argv[i], "--stats")) {
			stats = 1;
		} else if (!strcmp(argv[i], "--quiet")) {
			quiet = 1;
		} else {
			usage();
			return 2;
		}
	}

	/* Block SIGINT/SIGTERM everywhere so sigwait() is the only
	 * receiver; then hand the pipeline over to the fxpw worker. */
	sigset_t sigs;
	sigemptyset(&sigs);
	sigaddset(&sigs, SIGINT);
	sigaddset(&sigs, SIGTERM);
	pthread_sigmask(SIG_BLOCK, &sigs, NULL);

	FXPW *pw = fxpw_start(sink_name, have_preset ? preset_path : NULL,
			      out_name, make_default, bypass, stats, quiet,
			      err, sizeof(err));
	if (pw == NULL) {
		fprintf(stderr, "saucefxrt: %s\n", err);
		return 1;
	}

	int sig = 0;
	sigwait(&sigs, &sig);
	if (!quiet)
		fprintf(stderr, "saucefxrt: signal %d, stopping\n", sig);

	long long frames, dropped, underruns;
	int rate, ch, state, ring_fr;
	fxpw_get_stats(pw, &frames, &dropped, &underruns, &rate, &ch,
		       &state, &ring_fr);
	fprintf(stderr, "\nsaucefxrt: done.\n");
	fprintf(stderr, "  frames processed : %lld (%.1f s @ %d Hz)\n",
		frames, rate > 0 ? frames / (double)rate : 0.0, rate);
	fprintf(stderr, "  frames dropped   : %lld\n", dropped);
	fprintf(stderr, "  underruns        : %lld\n", underruns);

	fxpw_stop(pw);
	return 0;
}
