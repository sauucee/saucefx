/*
 * Regression coverage for EQ control versus audio-thread registry polling.
 *
 * This test deliberately needs no PipeWire server: one thread drives the
 * real DfxDsp audio callback while the main thread performs the same 31-band
 * updates used by the GUI. It also verifies that an immediately saved preset
 * round-trips the current (not previously selected) EQ gains.
 */

#include <DfxDsp.h>

#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <string>
#include <thread>
#include <vector>

#include <unistd.h>

namespace fs = std::filesystem;
using namespace std::chrono_literals;

namespace {

constexpr int kMaxBands = 32;
constexpr int kFramesPerBuffer = 256;

class AudioPump
{
public:
	explicit AudioPump(DfxDsp &dsp) : dsp_(dsp), thread_(&AudioPump::run, this) {}

	~AudioPump()
	{
		stop_.store(true, std::memory_order_release);
		pause_requested_.store(false, std::memory_order_release);
		if (thread_.joinable())
			thread_.join();
	}

	unsigned long buffers() const
	{
		return buffers_.load(std::memory_order_acquire);
	}

	bool waitForBuffers(unsigned long target)
	{
		auto deadline = std::chrono::steady_clock::now() + 3s;
		while (buffers() < target && !failed_.load(std::memory_order_acquire)) {
			if (std::chrono::steady_clock::now() >= deadline)
				return false;
			std::this_thread::sleep_for(100us);
		}
		return !failed_.load(std::memory_order_acquire);
	}

	bool pause()
	{
		pause_requested_.store(true, std::memory_order_release);
		auto deadline = std::chrono::steady_clock::now() + 3s;
		while (!paused_.load(std::memory_order_acquire) &&
		       !failed_.load(std::memory_order_acquire)) {
			if (std::chrono::steady_clock::now() >= deadline)
				return false;
			std::this_thread::sleep_for(100us);
		}
		return !failed_.load(std::memory_order_acquire);
	}

	void resume()
	{
		pause_requested_.store(false, std::memory_order_release);
		/* Do not let the next control update outrun the worker's resume;
		 * each iteration should genuinely overlap a live audio callback. */
		while (paused_.load(std::memory_order_acquire) &&
		       !failed_.load(std::memory_order_acquire))
			std::this_thread::sleep_for(50us);
	}

private:
	void run()
	{
		std::vector<float> audio(kFramesPerBuffer * 2, 0.0f);
		while (!stop_.load(std::memory_order_acquire)) {
			if (pause_requested_.load(std::memory_order_acquire)) {
				paused_.store(true, std::memory_order_release);
				while (pause_requested_.load(std::memory_order_acquire) &&
				       !stop_.load(std::memory_order_acquire))
					std::this_thread::sleep_for(50us);
				paused_.store(false, std::memory_order_release);
				continue;
			}

			int rc = dsp_.processAudio(
				reinterpret_cast<short int *>(audio.data()),
				reinterpret_cast<short int *>(audio.data()),
				kFramesPerBuffer, false);
			if (rc != 0) {
				failed_.store(true, std::memory_order_release);
				return;
			}
			buffers_.fetch_add(1, std::memory_order_release);
			std::this_thread::yield();
		}
	}

	DfxDsp &dsp_;
	std::atomic<bool> stop_{false};
	std::atomic<bool> pause_requested_{false};
	std::atomic<bool> paused_{false};
	std::atomic<bool> failed_{false};
	std::atomic<unsigned long> buffers_{0};
	std::thread thread_;
};

bool checkGains(DfxDsp &dsp, const std::array<float, kMaxBands> &expected,
		int bands, const char *phase, int iteration)
{
	for (int band = 0; band < bands; ++band) {
		float actual = dsp.getEqBandBoostCut(band);
		if (std::fabs(actual - expected[band]) > 0.26f) {
			std::fprintf(stderr,
				"FAIL %s iteration %d band %d: expected %.2f, got %.2f\n",
				phase, iteration, band, expected[band], actual);
			return false;
		}
	}
	return true;
}

void setPattern(DfxDsp &dsp, std::array<float, kMaxBands> *expected,
		int bands, int seed)
{
	for (int band = 0; band < bands; ++band) {
		(*expected)[band] =
			static_cast<float>(((seed * 7 + band * 13) % 25) - 12) * 0.5f;
		dsp.setEqBandBoostCut(band, (*expected)[band]);
	}
}

} /* namespace */

int main(int argc, char **argv)
{
	int iterations = argc > 1 ? std::atoi(argv[1]) : 250;
	if (iterations < 1)
		iterations = 1;

	char temp_pattern[] = "/tmp/saucefx-eq-registry-test-XXXXXX";
	char *temp_dir = mkdtemp(temp_pattern);
	if (temp_dir == nullptr) {
		std::perror("mkdtemp");
		return 1;
	}
	fs::path work_dir(temp_dir);
	fs::path registry = work_dir / "registry.txt";
	if (setenv("FXSOUND_REG_FILE", registry.c_str(), 1) != 0) {
		std::perror("setenv");
		fs::remove_all(work_dir);
		return 1;
	}

	int result = 0;
	{
		DfxDsp dsp;
		if (dsp.setSignalFormat(32, 2, 48000, 32) != 0) {
			std::fprintf(stderr, "FAIL setSignalFormat\n");
			fs::remove_all(work_dir);
			return 1;
		}
		dsp.powerOn(true);

		int bands = dsp.getNumEqBands();
		if (bands < 1 || bands > kMaxBands) {
			std::fprintf(stderr, "FAIL invalid EQ band count: %d\n", bands);
			fs::remove_all(work_dir);
			return 1;
		}

		AudioPump pump(dsp);
		std::array<float, kMaxBands> expected{};
		int mismatches = 0;
		if (!pump.waitForBuffers(1)) {
			std::fprintf(stderr, "FAIL audio pump did not start\n");
			fs::remove_all(work_dir);
			return 1;
		}

		for (int iteration = 0; iteration < iterations; ++iteration) {
			unsigned long before = pump.buffers();
			setPattern(dsp, &expected, bands, iteration);
			if (!pump.waitForBuffers(before + 3) || !pump.pause()) {
				std::fprintf(stderr, "FAIL audio pump stalled during race test\n");
				result = 1;
				break;
			}
			if (!checkGains(dsp, expected, bands, "registry race", iteration))
				++mismatches;
			pump.resume();
		}

		if (mismatches != 0) {
			std::fprintf(stderr, "FAIL mismatched iterations: %d/%d\n",
				mismatches, iterations);
			result = 1;
		} else if (result == 0) {
			std::printf("PASS registry race: 0/%d mismatched iterations\n",
				iterations);
		}

		/* Capture freshly edited gains immediately, then prove that loading
		 * the generated preset restores exactly that coherent snapshot. */
		pump.resume();
		setPattern(dsp, &expected, bands, iterations + 17);
		std::wstring preset_name = L"EQ roundtrip";
		std::string preset_dir_utf8 = work_dir.string();
		std::wstring preset_dir(preset_dir_utf8.begin(), preset_dir_utf8.end());
		if (dsp.savePreset(preset_name, preset_dir) != 0) {
			std::fprintf(stderr, "FAIL savePreset\n");
			result = 1;
		} else {
			fs::path preset_path = work_dir / "EQ roundtrip.fac";
			if (!fs::exists(preset_path) || fs::file_size(preset_path) == 0) {
				std::fprintf(stderr, "FAIL saved preset is missing or empty\n");
				result = 1;
			} else {
				std::array<float, kMaxBands> changed{};
				setPattern(dsp, &changed, bands, iterations + 41);
				std::string preset_path_utf8 = preset_path.string();
				std::wstring wide_path(preset_path_utf8.begin(),
					preset_path_utf8.end());
				if (dsp.loadPreset(wide_path) != 0) {
					std::fprintf(stderr, "FAIL loadPreset roundtrip\n");
					result = 1;
				} else {
					unsigned long before = pump.buffers();
					if (!pump.waitForBuffers(before + 3) || !pump.pause()) {
						std::fprintf(stderr,
							"FAIL audio pump stalled during preset roundtrip\n");
						result = 1;
					} else {
						if (!checkGains(dsp, expected, bands,
								"preset roundtrip", 0))
							result = 1;
						DfxPreset info = dsp.getPresetInfo(wide_path);
						if (info.name != preset_name) {
							std::fprintf(stderr,
								"FAIL preset name did not round-trip\n");
							result = 1;
						}
						pump.resume();
					}
				}
			}
		}

		if (result == 0)
			std::printf("PASS preset save/load preserved all %d EQ gains\n", bands);
	}

	fs::remove_all(work_dir);
	return result;
}
