/* efftest2: does a runtime setEffectValue() reach the DSP programs?
 * Defeats the duplicate-buffer hash skip by using disjoint buffers and
 * a 1-LSB perturbation. Pass1 = first half w/ preset defaults, push all
 * effects to max, Pass2 = second half, push ambience+fidelity to 0,
 * Pass3 = second half again (perturbed by 1 LSB) => out3 vs out2 is the
 * knob-comms verdict. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <filesystem>
#include <string>
#include <vector>
#include <DfxDsp.h>

static bool readWave16(const std::string &path, std::vector<short int> &out)

{
	FILE *f = fopen(path.c_str(), "rb");
	if (f == NULL)
		return false;
	unsigned char hdr[44];
	if (fread(hdr, 1, 44, f) != 44) { fclose(f); return false; }
	fseek(f, 0, SEEK_END);
	long sz = ftell(f) - 44;
	fseek(f, 44, SEEK_SET);
	size_t n = (size_t)sz / 2;
	out.resize(n);
	if (fread(out.data(), 1, (size_t)sz, f) != (size_t)sz) {
		fclose(f);
		return false;
	}
	fclose(f);
	return true;
}

static bool writeWave16(const std::string &path, const std::vector<short int> &in)
{
	FILE *f = fopen(path.c_str(), "wb");
	if (f == NULL)
		return false;
	unsigned char hdr[44];
	memset(hdr, 0, sizeof(hdr));
	long dat = (long)in.size() * 2;
	memcpy(hdr, "RIFF", 4);
	long riff = 36 + dat;
	hdr[4] = (unsigned char)(riff); hdr[5] = (unsigned char)(riff >> 8);
	hdr[6] = (unsigned char)(riff >> 16); hdr[7] = (unsigned char)(riff >> 24);
	memcpy(hdr + 8, "WAVE", 4);
	memcpy(hdr + 12, "fmt ", 4);
	hdr[16] = 16; hdr[20] = 1; hdr[22] = 2;
	hdr[24] = (unsigned char)(48000); hdr[25] = (unsigned char)(48000 >> 8);
	hdr[28] = 4; hdr[32] = 4; hdr[34] = 16;
	memcpy(hdr + 36, "data", 4);
	hdr[40] = (unsigned char)(dat); hdr[41] = (unsigned char)(dat >> 8);
	hdr[42] = (unsigned char)(dat >> 16); hdr[43] = (unsigned char)(dat >> 24);
	fwrite(hdr, 1, 44, f);
	fwrite(in.data(), 2, in.size(), f);
	fclose(f);
	return true;
}

int main(int argc, char **argv)
{
	if (argc < 3) {
		printf("usage: efftest2 in.wav preset.fac\n");
		return 1;
	}

	const auto temp = std::filesystem::temp_directory_path();
	std::vector<short int> samples;
	fprintf(stderr, "efftest: reading %s\n", argv[1]);
	if (!readWave16(argv[1], samples)) {
		fprintf(stderr, "FAIL read\n");
		return 1;
	}
	fprintf(stderr, "efftest: read %zu samples\n", samples.size());
	long frames = (long)samples.size() / 2;
	long h1len = (frames / 3) * 2;   /* pass1 = first 2/3 in frames*2 shorts... see below */

	/* h1 = first half, h2 = second half (disjoint, unique content) */
	std::vector<short int> h1(samples.begin(), samples.begin() + samples.size() / 2);
	std::vector<short int> h2(samples.begin() + samples.size() / 2, samples.end());

	DfxDsp dsp;
	fprintf(stderr, "efftest: dsp created\n");
	if (dsp.setSignalFormat(16, 2, 48000, 16) != 0) {
		fprintf(stderr, "FAIL setSignalFormat\n");
		return 1;
	}
	fprintf(stderr, "efftest: setSignalFormat ok\n");

	wchar_t pbuf[512];
	size_t t = mbstowcs(pbuf, argv[2], sizeof(pbuf) / sizeof(pbuf[0]) - 1);
	pbuf[t] = 0;
	fprintf(stderr, "efftest: loading preset\n");
	if (dsp.loadPreset(pbuf) != 0) {
		fprintf(stderr, "FAIL loadPreset\n");
		return 1;
	}
	fprintf(stderr, "efftest: preset ok\n");

	std::vector<short int> out1(h1.size());
	fprintf(stderr, "===PASS1-START===\n");
	int rc1 = dsp.processAudio(h1.data(), out1.data(), (int)(h1.size() / 2), 0);
	fprintf(stderr, "===PASS1-END===\n");
	double s1 = 0.0;
	for (size_t k = 0; k < out1.size(); k++)
		s1 += ((double)out1[k] / 32768.0) * ((double)out1[k] / 32768.0);
	fprintf(stderr, "pass1 rc=%d rms=%.5f\n", rc1, sqrt(s1 / out1.size()));
	writeWave16((temp / "saucefx-eff-out1.wav").string(), out1);

	/* Push ALL five effects to max */
	fprintf(stderr, "===PUSH-MAX===\n");
	static const DfxDsp::Effect order[5] = {
		DfxDsp::Effect::Fidelity, DfxDsp::Effect::Ambience,
		DfxDsp::Effect::Surround, DfxDsp::Effect::DynamicBoost,
		DfxDsp::Effect::Bass
	};
	for (int i = 0; i < 5; i++)
		dsp.setEffectValue(order[i], 10.0f);

	printf("readback after push:");
	for (int i = 0; i < 5; i++)
		printf(" %g", dsp.getEffectValue(order[i]) / 10.0f);
	printf("\n");
	fflush(stdout);

	std::vector<short int> out2(h2.size());
	fprintf(stderr, "===PASS2-START===\n");
	dsp.processAudio(h2.data(), out2.data(), (int)(h2.size() / 2), 0);
	fprintf(stderr, "===PASS2-END===\n");
	double s2 = 0.0;
	for (size_t k = 0; k < out2.size(); k++)
		s2 += ((double)out2[k] / 32768.0) * ((double)out2[k] / 32768.0);
	writeWave16((temp / "saucefx-eff-out2.wav").string(), out2);

	/* Push fidelity and ambience to ZERO (big setters), keep others max */
	fprintf(stderr, "===PUSH-ZERO2===\n");
	dsp.setEffectValue(DfxDsp::Effect::Fidelity, 0.0f);
	dsp.setEffectValue(DfxDsp::Effect::Ambience, 0.0f);

	/* Pass3: h2 again but perturbed by 1 LSB in ONE sample so the
	 * duplicate-buffer hash differs, keeping the audio equivalent. */
	std::vector<short int> h2b = h2;
	if (!h2b.empty())
		h2b[h2b.size() / 2] = (short int)(h2b[h2b.size() / 2] + 1);

	std::vector<short int> out3(h2b.size());
	fprintf(stderr, "===PASS3-START===\n");
	dsp.processAudio(h2b.data(), out3.data(), (int)(h2b.size() / 2), 0);
	fprintf(stderr, "===PASS3-END===\n");
	writeWave16((temp / "saucefx-eff-out3.wav").string(), out3);

	double d32 = 0.0, s3 = 0.0;
	for (size_t k = 0; k < out3.size(); k++) {
		double a = (double)out2[k] / 32768.0;
		double b = (double)out3[k] / 32768.0;
		d32 += (b - a) * (b - a);
		s3 += b * b;
	}
	fprintf(stderr, "rms2=%.5f rms3=%.5f delta(out3-out2)=%.5f\n",
		sqrt(s2 / out2.size()), sqrt(s3 / out3.size()),
		sqrt(d32 / out3.size()));

	/* NOTE: ~DfxDspPrivate segfaults in prelstFreeUp on this path; leak
	 * the handle so measurements reach stdout first. */
	fflush(stdout);
	_Exit(0);
}
