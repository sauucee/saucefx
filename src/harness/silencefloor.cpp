/* Measure the real DSP's residual on digital-zero input, before the output is
 * quantized by a playback device. Diagnostic only: these figures are not CPU
 * benchmarks. Use the same init, preset and in-place float ABI as the bridge. */
#include "../pwbridge/fxpw.cpp"
#include <algorithm>
#include <array>
#include <cmath>
#include <filesystem>

// As in audiolatencytest, replace only the transport endpoints. The callback,
// preset, per-buffer setup, real processor and its output buffer stay intact.
static pw_buffer buffer{};
extern "C" pw_buffer *pw_stream_dequeue_buffer(pw_stream *) { return &buffer; }
extern "C" int pw_stream_queue_buffer(pw_stream *, pw_buffer *b) { return b == &buffer ? 0 : -1; }

int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    constexpr int rate = 48000, frames = 256, channels = 2;
    DfxDsp dsp;
    FXPW bridge{};
    bridge.dsp = &dsp;
    bridge.pending_preset_path = std::filesystem::path(argv[1]).wstring();
    bridge.pending_preset = true;
    bridge.power_on = true;
    std::array<float, frames * channels> samples{};
    bridge.rate = rate;
    bridge.ch = channels;
    spa_chunk chunk{}; chunk.size = samples.size() * sizeof(float);
    spa_data data{}; data.data = samples.data(); data.chunk = &chunk; data.maxsize = chunk.size;
    spa_buffer storage{}; storage.n_datas = 1; storage.datas = &data;
    buffer.buffer = &storage;
    for (int phase = 0; phase < 3; ++phase) {
        const bool tone = phase == 1;
        const int blocks = (tone ? 2 : 8) * rate / frames;
        int exact_zero_blocks = 0, skipped = 0;
        double peak = 0.0;
        for (int b = 0; b < blocks; ++b) {
            samples.fill(0.0f);
            if (tone) for (int i = 0; i < frames; ++i) {
                const float v = 0.25f * std::sin(2.0 * 3.141592653589793 * 440 * (b * frames + i) / rate);
                samples[2*i] = samples[2*i+1] = v;
            }
            capture_process(&bridge);
            if (b >= blocks - rate / frames) {
                skipped += bridge.gate.skipping;
                if (saucefx_gate_zero(samples.data(), samples.size())) ++exact_zero_blocks;
                for (float v : samples) { if (!std::isfinite(v)) return 1; peak = std::max(peak, std::fabs(double(v))); }
            }
        }
        std::printf("DSP_RESIDUE phase=%s exact_zero_blocks=%d/%d skipped=%d peak=%.10e\n", phase==0 ? "cold" : tone ? "tone" : "after_playback", exact_zero_blocks, rate/frames, skipped, peak);
        if (tone && peak < 0.01) return 1;
    }
}
