/* Minimal probe mirroring AudioPassthruPrivate.cpp processingThread:
 * 32-bit float buffers, in-place, setSignalFormat(32,...) each iteration. */
#include <DfxDsp.h>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cmath>

#include <unistd.h>
#include <string>
#include <vector>

int main()
{
	std::string reg = "/tmp/probe-reg-XXXXXX";
	std::vector<char> rb(reg.begin(), reg.end()); rb.push_back(0);
	int fd = mkstemp(rb.data()); close(fd);
	setenv("FXSOUND_REG_FILE", rb.data(), 1);
	unlink(rb.data());

	DfxDsp dsp;
	const int N = 1024;
	static float buf[2*N];

	dsp.powerOn(true);
	float vals[6] = {0.48f, 0.36f, 0.45f, 0.27f, 0.5f, 0.33f};
	for (int chunk = 0; chunk < 6; chunk++) {
		if (dsp.setSignalFormat(32, 2, 44100, 32) != 0) { printf("setSignalFormat fail\n"); return 1; }
		/* start of buffer: loop pattern so each chunk differs */
		float v = (float)(0.4 + 0.05*chunk);
		for (int i = 0; i < N; i++) { buf[2*i] = v; buf[2*i+1] = -v; }
		if (dsp.processAudio((short int *)buf, (short int *)buf, N, false) != 0) { printf("processAudio fail\n"); return 1; }
		long sum = 0; int mx = 0;
		short* s = (short*)buf;
		(void)s;
		printf("chunk %d (in=%.2f): out[0..9]: ", chunk, v);
		for (int i = 0; i < 10; i++) printf("%.4f ", buf[i]);
		printf(" ... mean|out|=%.4f max=%.4f\n",
			[&]{ long long t=0; for (int i=0;i<2*N;i++) t += (long long)fabs(buf[i]*1e6); return t/(2*N); }() / 1e6,
			[&]{ float m=0; for (int i=0;i<2*N;i++) { float a = buf[i]<0?-buf[i]:buf[i]; if (a>m) m=a; } return m; }());
	}
	return 0;
}
