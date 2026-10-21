/* Headless verification of the route-toggle restore logic in fxpw.cpp.
 *
 * Scenario that broke in the field: the system default already points at
 * the virtual sink when the user toggles route ON (leftover from an
 * earlier session) - the old code then marked did_default=0, and the
 * OFF toggle had nothing to restore, leaving the system permanently on
 * the virtual sink. This test drives the REAL fxpw API through all the
 * cases.
 */
#include <fxpw.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static int is_routed(FXPW *h)
{
	return fxpw_is_routed_default(h);
}

/* The default change settles asynchronously in pipewire-pulse; bless
 * decision point by polling up to 2s instead of racing one instant
 * read. Returns the final observed routed state. */
static int settled_routed(FXPW *h, int expected)
{
	for (int i = 0; i < 40; i++) {
		int r = fxpw_is_routed_default(h);
		if (r == expected)
			return r;
		usleep(50000);
	}
	return fxpw_is_routed_default(h);
}

int main(void)
{
	char err[192] = "";
	FXPW *h = fxpw_start("FxSound", NULL, NULL,
			     /* make_default */ 0, /* bypass */ 0,
			     /* stats */ 0, /* quiet */ 0, err, sizeof(err));
	if (h == NULL) {
		fprintf(stderr, "START FAILED: %s\n", err);
		return 1;
	}

	int fails = 0;

	/* 1. Normal toggle on/off. */
	fprintf(stderr, "t1 start: routed=%d\n", is_routed(h));
	fxpw_route_default(h, 1);
	if (!settled_routed(h, 1)) { fprintf(stderr, "FAIL t1: ON did not route\n"); fails++; }
	fxpw_route_default(h, 0);
	if (settled_routed(h, 0)) { fprintf(stderr, "FAIL t1: OFF did not restore\n"); fails++; }
	fprintf(stderr, "t1 done: routed=%d (expect 0)\n", is_routed(h));

	/* 2. Leftover case: default stuck on the virtual sink, session
	 * never routed. OFF must still heal it. */
	int preload = system("pactl set-default-sink FxSound");
	(void)preload;
	fprintf(stderr, "t2 forced default onto virtual sink: routed=%d (expect 1)\n",
		settled_routed(h, 1));
	fxpw_route_default(h, 0);
	if (settled_routed(h, 0)) {
		fprintf(stderr, "FAIL t2: OFF did not heal stray default\n");
		fails++;
	}
	fprintf(stderr, "t2 done: routed=%d (expect 0)\n", is_routed(h));

	/* 3. Toggle ON while already routed (idempotent), then OFF. */
	fxpw_route_default(h, 1);
	fxpw_route_default(h, 1);
	if (!settled_routed(h, 1)) { fprintf(stderr, "FAIL t3: ON-on lost routing\n"); fails++; }
	fxpw_route_default(h, 0);
	if (settled_routed(h, 0)) { fprintf(stderr, "FAIL t3: OFF did not restore\n"); fails++; }
	fprintf(stderr, "t3 done: routed=%d (expect 0)\n", is_routed(h));

	fxpw_stop(h);
	fprintf(stderr, "%s (%d failures)\n", fails ? "FAILED" : "PASSED", fails);
	return fails ? 1 : 0;
}
