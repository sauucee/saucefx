/* Live output/port discovery through PipeWire's PulseAudio compatibility server.
 * Audio processing still uses native PipeWire streams. */
#ifndef SAUCEFX_OUTPUTS_H
#define SAUCEFX_OUTPUTS_H

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace saucefx {

struct OutputPort {
	std::string name;
	std::string description;
	bool available = true; /* unknown jack availability is usable */
	uint32_t priority = 0;
	bool operator==(const OutputPort &other) const;
};

struct OutputDevice {
	std::string name;
	std::string description;
	std::string activePort;
	std::vector<OutputPort> ports;
	int priority = 0;
	bool external = false;
	bool available(const std::string &port = {}) const;
	bool operator==(const OutputDevice &other) const;
};

struct OutputSnapshot {
	std::vector<OutputDevice> devices;
	std::string defaultSink;
	std::string automaticSink; /* last physical default, never SauceFX */
	bool connected = false;
};

/* A missing preferred device/port temporarily falls back. Keeping the preference
 * separate from the active target lets a Bluetooth/USB device reconnect. */
const OutputDevice *chooseOutput(const OutputSnapshot &snapshot,
				const std::string &preferred,
				const std::string &port = {});

class OutputMonitor {
public:
	explicit OutputMonitor(const std::string &virtualSink);
	~OutputMonitor();
	bool start(std::string &error);
	OutputSnapshot snapshot() const;
	bool setDefault(const std::string &name);
	bool setPort(const std::string &name, const std::string &port);
	/* Non-blocking operations for the PipeWire control loop. */
	void requestDefault(const std::string &name);
	void requestPort(const std::string &name, const std::string &port);

private:
	struct Impl;
	std::unique_ptr<Impl> impl;
};

} // namespace saucefx
#endif
