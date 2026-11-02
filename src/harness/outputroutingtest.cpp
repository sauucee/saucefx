/* Integration regression for physical output discovery, hot-plug and routing.
 * Run only through outputroutingtest.sh, on its private PipeWire/Pulse server. */
#include "fxpw.h"
#include "outputs.h"

#include <chrono>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <vector>

static void require(bool condition, const std::string &message)
{
	if (!condition)
		throw std::runtime_error(message);
}

static std::string capture(const std::string &command)
{
	/* Fixture setup/teardown is privileged; the bridge must independently
	 * declare its manager role on its own connection. */
	FILE *pipe = popen(("env PULSE_PROP=media.category=Manager " + command).c_str(), "r");
	require(pipe != nullptr, "cannot run " + command);
	std::string result;
	char line[512];
	while (fgets(line, sizeof(line), pipe))
		result += line;
	require(pclose(pipe) == 0, "command failed: " + command);
	while (!result.empty() && (result.back() == '\n' || result.back() == '\r'))
		result.pop_back();
	return result;
}

static void waitFor(const std::string &message, const std::function<bool()> &condition)
{
	const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(8);
	do {
		if (condition()) {
			printf("PASS: %s\n", message.c_str());
			return;
		}
		std::this_thread::sleep_for(std::chrono::milliseconds(50));
	} while (std::chrono::steady_clock::now() < deadline);
	throw std::runtime_error("timed out: " + message);
}

static std::string addSink(const char *name, int priority, const char *bus = "pci")
{
	return capture(std::string("pactl load-module module-null-sink sink_name=") + name +
		" sink_properties='device.description=" + name + " device.bus=" + bus +
		" priority.session=" + std::to_string(priority) + "'");
}

static void removeSink(const std::string &module)
{
	capture("pactl unload-module " + module);
}

static bool listed(FXPW *engine, const char *name)
{
	std::vector<FXPWOutput> devices(32);
	const auto count = fxpw_get_outputs(engine, devices.data(), devices.size());
	for (size_t i = 0; i < count && i < devices.size(); ++i)
		if (devices[i].name == std::string(name))
			return true;
	return false;
}

static void targetIs(FXPW *engine, const std::string &name)
{
	waitFor("render links target " + (name.empty() ? std::string("no device") : name), [&] {
		FXPWOutputState state{};
		fxpw_get_output_state(engine, &state);
		if (state.active_name != name)
			return false;
		return system(("python3 ./harness/outputgraphtest.py " +
			(name.empty() ? "none" : name)).c_str()) == 0;
	});
}

static void policyTests()
{
	saucefx::OutputSnapshot snapshot;
	snapshot.connected = true;
	saucefx::OutputDevice internal;
	internal.name = "built-in";
	internal.ports = {{"lineout", "Line Out", true, 9000},
		{"headphones", "Headphones", false, 9900}};
	internal.activePort = "lineout";
	require(internal.available() && !internal.available("headphones"),
		"unplugged headphone jack must be unavailable");
	internal.ports[1].available = true;
	require(internal.available("headphones"), "plugged headphone jack must be selectable");
	snapshot.devices = {internal};
	snapshot.automaticSink = internal.name;
	require(saucefx::chooseOutput(snapshot, "missing-headset")->name == internal.name,
		"missing preferred headset must fall back");
	snapshot.devices[0].ports[0].available = false;
	snapshot.devices[0].ports[1].available = false;
	require(saucefx::chooseOutput(snapshot, "built-in") == nullptr,
		"sink with no available ports must not be rendered to");
	snapshot.devices[0].ports.clear();
	require(saucefx::chooseOutput(snapshot, "built-in") != nullptr,
		"portless USB/Bluetooth sinks must be usable");
	snapshot.connected = false;
	require(saucefx::chooseOutput(snapshot, "built-in") == nullptr,
		"disconnected monitor must not expose a render target");
	puts("PASS: wired jack availability and missing-device fallback policy");
}

int main()
{
	try {
		require(getenv("SAUCEFX_TEST_ISOLATED") != nullptr,
			"use outputroutingtest.sh to isolate audio from the desktop");
		policyTests();
		auto first = addSink("Test_First", 2000);
		auto second = addSink("Test_Default", 1000);
		capture("pactl set-default-sink Test_Default");
		require(system("pactl --client-name='SauceFX outputs' set-default-sink Test_First >/dev/null 2>&1") != 0,
			"test server must deny default changes to a non-manager client");
		puts("PASS: sandbox-style restrictions reject non-manager output control");
		char error[256]{};
		using Engine = std::unique_ptr<FXPW, decltype(&fxpw_stop)>;
		Engine engine(fxpw_start("SauceFXDeviceTest", nullptr, nullptr,
			1, 0, 0, 0, error, sizeof(error)), fxpw_stop);
		require(engine != nullptr, error);
		targetIs(engine.get(), "Test_Default");
		require(listed(engine.get(), "Test_First") && listed(engine.get(), "Test_Default") &&
			!listed(engine.get(), "SauceFXDeviceTest"), "live output list must exclude its virtual sink");
		require(fxpw_set_output(engine.get(), "SauceFXDeviceTest", "") != 0,
			"self-feedback selection must be rejected");
		fxpw_set_eq_band(engine.get(), 17, fxpw_get_eq_band_freq(engine.get(), 17), 3.25f);
		fxpw_set_effect_value(engine.get(), 0, 4.0f);
		require(fxpw_set_output(engine.get(), "Test_First", "") == 0, "select first output");
		targetIs(engine.get(), "Test_First");
		auto usb = addSink("Test_USB", 3000, "usb");
		waitFor("USB hot-plug appears in output list", [&] { return listed(engine.get(), "Test_USB"); });
		std::this_thread::sleep_for(std::chrono::milliseconds(300));
		targetIs(engine.get(), "Test_First");
		require(fxpw_set_output(engine.get(), "", "") == 0, "switch to automatic mode");
		targetIs(engine.get(), "Test_USB");
		capture("pactl set-default-sink Test_Default");
		targetIs(engine.get(), "Test_Default");
		waitFor("OS default change keeps system audio processed", [&] {
			return capture("pactl get-default-sink") == "SauceFXDeviceTest";
		});
		removeSink(usb);
		auto bluetooth = addSink("Test_Bluetooth", 4000, "bluetooth");
		targetIs(engine.get(), "Test_Bluetooth");
		require(fxpw_set_output(engine.get(), "Test_Bluetooth", "") == 0, "remember headset");
		removeSink(bluetooth);
		waitFor("disconnected headset disappears", [&] { return !listed(engine.get(), "Test_Bluetooth"); });
		targetIs(engine.get(), "Test_First");
		FXPWOutputState state{};
		fxpw_get_output_state(engine.get(), &state);
		require(std::string(state.preferred_name) == "Test_Bluetooth", "retain headset preference");
		bluetooth = addSink("Test_Bluetooth", 4000, "bluetooth");
		targetIs(engine.get(), "Test_Bluetooth");
		require(std::fabs(fxpw_get_eq_band_boostcut(engine.get(), 17) - 3.25f) < 0.01f &&
			std::fabs(fxpw_get_effect_value(engine.get(), 0) - 4.0f) < 0.1f,
			"output changes must preserve DSP settings");
		puts("PASS: EQ and effects survive output switches and headset reconnect");
		removeSink(first);
		removeSink(second);
		removeSink(bluetooth);
		targetIs(engine.get(), "");
		require(fxpw_route_default(engine.get(), 0) == 0, "disable routing with no hardware");
		second = addSink("Test_Default", 1000);
		targetIs(engine.get(), "Test_Default");
		waitFor("route-off restores a newly available physical output", [&] {
			return capture("pactl get-default-sink") == "Test_Default";
		});
		require(fxpw_route_default(engine.get(), 1) == 0, "reenable routing");
		engine.reset();
		waitFor("shutdown restores connected output", [&] {
			return capture("pactl get-default-sink") == "Test_Default";
		});
		removeSink(second);
		engine.reset(fxpw_start("SauceFXDeviceTest", nullptr, nullptr,
			1, 0, 0, 0, error, sizeof(error)));
		require(engine != nullptr, error);
		targetIs(engine.get(), "");
		usb = addSink("Test_LateUSB", 3000, "usb");
		targetIs(engine.get(), "Test_LateUSB");
		engine.reset();
		removeSink(usb);
		puts("PASS: start without hardware, then connect a late output");
		return 0;
	} catch (const std::exception &error) {
		fprintf(stderr, "FAIL: %s\n", error.what());
		return 1;
	}
}
