/* Probe the live EQ surface: band count, freqs, current gains. */
#include <fxpw.h>
#include <stdio.h>

int main(void)
{
	char err[192] = "";
	FXPW *h = fxpw_start("FxSoundProbe", NULL, NULL, 0, 0, 0, 1 /*quiet*/,
			     err, sizeof(err));
	if (h == NULL) {
		fprintf(stderr, "START FAILED: %s\n", err);
		return 1;
	}

	int nb = fxpw_get_num_eq_bands(h);
	fprintf(stderr, "num_eq_bands: %d\n", nb);
	for (int b = 0; b < nb; b++) {
		fprintf(stderr, "band %2d: freq=%8.1f Hz  gain=%+.2f dB\n", b,
			(double)fxpw_get_eq_band_freq(h, b),
			(double)fxpw_get_eq_band_boostcut(h, b));
	}

	fxpw_stop(h);
	return 0;
}
