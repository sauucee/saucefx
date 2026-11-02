#include "outputs.h"

#include <pulse/pulseaudio.h>

#include <algorithm>
#include <chrono>
#include <condition_variable>
#include <cstdlib>
#include <functional>
#include <mutex>
#include <tuple>

namespace saucefx {

bool OutputPort::operator==(const OutputPort &other) const
{
	return std::tie(name, description, available, priority) ==
	       std::tie(other.name, other.description, other.available,
			other.priority);
}

bool OutputDevice::operator==(const OutputDevice &other) const
{
	return std::tie(name, description, activePort, ports, priority, external) ==
	       std::tie(other.name, other.description, other.activePort,
			other.ports, other.priority, other.external);
}

bool OutputDevice::available(const std::string &port) const
{
	if (!port.empty())
		return std::any_of(ports.begin(), ports.end(), [&](const auto &p) {
			return p.name == port && p.available;
		});
	return ports.empty() ||
	       std::any_of(ports.begin(), ports.end(), [](const auto &p) {
		       return p.available;
	       });
}

const OutputDevice *chooseOutput(const OutputSnapshot &snapshot,
				const std::string &preferred,
				const std::string &port)
{
	if (!snapshot.connected)
		return nullptr;
	const OutputDevice *best = nullptr;
	const OutputDevice *automatic = nullptr;
	for (const auto &device : snapshot.devices) {
		if (!preferred.empty() && device.name == preferred &&
		    device.available(port))
			return &device;
		if (!device.available())
			continue;
		if (device.name == snapshot.automaticSink)
			automatic = &device;
		if (best == nullptr || device.priority > best->priority)
			best = &device;
	}
	return automatic != nullptr ? automatic : best;
}

struct OutputMonitor::Impl {
	pa_threaded_mainloop *loop = nullptr;
	pa_context *context = nullptr;
	bool running = false;
	bool listing = false;
	bool refreshPending = false;
	bool gotDevices = false;
	bool gotServer = false;
	bool failed = false;
	std::string virtualSink;
	std::vector<OutputDevice> pendingDevices;
	mutable std::mutex mutex;
	std::condition_variable ready;
	OutputSnapshot data;

	explicit Impl(std::string name) : virtualSink(std::move(name)) {}

	~Impl()
	{
		if (running)
			pa_threaded_mainloop_stop(loop);
		if (context != nullptr) {
			pa_context_disconnect(context);
			pa_context_unref(context);
		}
		if (loop != nullptr)
			pa_threaded_mainloop_free(loop);
	}

	static std::string property(pa_proplist *props, const char *key)
	{
		const char *value = pa_proplist_gets(props, key);
		return value != nullptr ? value : "";
	}

	static void serverInfo(pa_context *, const pa_server_info *info, void *userdata)
	{
		auto &self = *static_cast<Impl *>(userdata);
		if (info == nullptr)
			return;
		{
			std::lock_guard<std::mutex> lock(self.mutex);
			std::string name = info->default_sink_name != nullptr
					   ? info->default_sink_name : "";
			/* Repeated info replies while SauceFX takes over must not
			 * overwrite the last physical default or a newly plugged device. */
			if (name != self.data.defaultSink && !name.empty() &&
			    name != self.virtualSink && name != "SauceFX" &&
			    name != "auto_null")
				self.data.automaticSink = name;
			self.data.defaultSink = std::move(name);
			self.gotServer = true;
		}
		self.ready.notify_all();
	}

	static void sinkInfo(pa_context *, const pa_sink_info *info, int eol,
			     void *userdata)
	{
		auto &self = *static_cast<Impl *>(userdata);
		if (eol != 0) {
			if (eol > 0) {
				std::sort(self.pendingDevices.begin(), self.pendingDevices.end(),
					  [](const auto &a, const auto &b) { return a.name < b.name; });
				std::lock_guard<std::mutex> lock(self.mutex);
				/* A wired jack is a port on an existing ALSA sink. USB and
				 * Bluetooth sinks, however, can appear after we became default.
				 * Remember a newly available external sink in automatic mode. */
				const OutputDevice *plugged = nullptr;
				if (self.gotDevices) {
					for (const auto &device : self.pendingDevices) {
						if (!device.external || !device.available())
							continue;
						auto old = std::find_if(self.data.devices.begin(),
							self.data.devices.end(), [&](const auto &d) {
								return d.name == device.name && d.available();
							});
						if (old == self.data.devices.end() &&
						    (plugged == nullptr || device.priority > plugged->priority))
							plugged = &device;
					}
				}
				if (plugged != nullptr)
					self.data.automaticSink = plugged->name;
				self.data.devices = std::move(self.pendingDevices);
				self.gotDevices = true;
			}
			self.listing = false;
			self.ready.notify_all();
			if (self.refreshPending) {
				self.refreshPending = false;
				self.refresh();
			}
			return;
		}
		if (info == nullptr || info->name == nullptr)
			return;
		std::string name = info->name;
		if (name == self.virtualSink || name == "SauceFX" || name == "auto_null" ||
		    property(info->proplist, "device.description").find("(fxpw-") != std::string::npos)
			return;
		OutputDevice device;
		device.name = std::move(name);
		device.description = info->description != nullptr ? info->description : device.name;
		device.activePort = info->active_port != nullptr ? info->active_port->name : "";
		device.priority = std::atoi(property(info->proplist, "priority.session").c_str());
		auto bus = property(info->proplist, "device.bus");
		device.external = bus == "usb" || bus == "bluetooth" ||
			device.name.find("bluez_output.") == 0 ||
			device.name.find("alsa_output.usb-") == 0;
		for (uint32_t i = 0; i < info->n_ports; ++i) {
			const auto &port = *info->ports[i];
			device.ports.push_back({port.name,
				port.description != nullptr ? port.description : port.name,
				port.available != PA_PORT_AVAILABLE_NO, port.priority});
		}
		self.pendingDevices.push_back(std::move(device));
	}

	void refresh()
	{
		if (pa_context_get_state(context) != PA_CONTEXT_READY)
			return;
		if (listing) {
			refreshPending = true;
			return;
		}
		listing = true;
		pendingDevices.clear();
		if (auto *op = pa_context_get_sink_info_list(context, sinkInfo, this))
			pa_operation_unref(op);
		else
			listing = false;
		if (auto *op = pa_context_get_server_info(context, serverInfo, this))
			pa_operation_unref(op);
	}

	static void subscription(pa_context *, pa_subscription_event_type_t,
				 uint32_t, void *userdata)
	{
		static_cast<Impl *>(userdata)->refresh();
	}

	static void stateChanged(pa_context *context, void *userdata)
	{
		auto &self = *static_cast<Impl *>(userdata);
		auto state = pa_context_get_state(context);
		{
			std::lock_guard<std::mutex> lock(self.mutex);
			self.data.connected = state == PA_CONTEXT_READY;
			self.failed = state == PA_CONTEXT_FAILED || state == PA_CONTEXT_TERMINATED;
		}
		if (state == PA_CONTEXT_READY) {
			pa_context_set_subscribe_callback(context, subscription, &self);
			if (auto *op = pa_context_subscribe(context,
				    static_cast<pa_subscription_mask_t>(PA_SUBSCRIPTION_MASK_SINK |
					    PA_SUBSCRIPTION_MASK_SERVER | PA_SUBSCRIPTION_MASK_CARD),
				    nullptr, nullptr))
				pa_operation_unref(op);
			self.refresh();
		}
		self.ready.notify_all();
		pa_threaded_mainloop_signal(self.loop, 0);
	}

	struct Command {
		Impl *owner;
		bool done = false;
		bool success = false;
	};

	static void commandDone(pa_context *, int success, void *userdata)
	{
		auto &command = *static_cast<Command *>(userdata);
		command.success = success != 0;
		command.done = true;
		command.owner->refresh();
		pa_threaded_mainloop_signal(command.owner->loop, 0);
	}

	static void commandTimeout(pa_mainloop_api *, pa_time_event *,
				   const timeval *, void *userdata)
	{
		auto &command = *static_cast<Command *>(userdata);
		command.done = true;
		pa_threaded_mainloop_signal(command.owner->loop, 0);
	}

	bool execute(const std::function<pa_operation *(Command *)> &operation)
	{
		pa_threaded_mainloop_lock(loop);
		Command command{this};
		pa_operation *op = nullptr;
		if (pa_context_get_state(context) == PA_CONTEXT_READY)
			op = operation(&command);
		if (op != nullptr) {
			auto *timer = pa_context_rttime_new(context,
				pa_rtclock_now() + 3 * PA_USEC_PER_SEC, commandTimeout, &command);
			while (timer != nullptr && !command.done &&
			       pa_context_get_state(context) == PA_CONTEXT_READY)
				pa_threaded_mainloop_wait(loop);
			if (timer != nullptr)
				pa_threaded_mainloop_get_api(loop)->time_free(timer);
			pa_operation_cancel(op);
			pa_operation_unref(op);
		}
		pa_threaded_mainloop_unlock(loop);
		return command.success;
	}

	static void requestDone(pa_context *, int, void *userdata)
	{
		static_cast<Impl *>(userdata)->refresh();
	}
};

OutputMonitor::OutputMonitor(const std::string &virtualSink)
	: impl(std::make_unique<Impl>(virtualSink)) {}
OutputMonitor::~OutputMonitor() = default;

bool OutputMonitor::start(std::string &error)
{
	auto &s = *impl;
	s.loop = pa_threaded_mainloop_new();
	if (s.loop == nullptr) {
		error = "cannot create output device monitor";
		return false;
	}
	/* WirePlumber gives Flatpak playback clients read/execute access only.
	 * This connection manages defaults and jack ports, so declare its actual
	 * role just like other audio-control applications. Audio streams use
	 * their own capture/playback connection. */
	auto *properties = pa_proplist_new();
	if (properties == nullptr) {
		error = "cannot create output manager properties";
		return false;
	}
	pa_proplist_sets(properties, "media.category", "Manager");
	s.context = pa_context_new_with_proplist(pa_threaded_mainloop_get_api(s.loop),
		"SauceFX outputs", properties);
	pa_proplist_free(properties);
	if (s.context == nullptr) {
		error = "cannot create PulseAudio client";
		return false;
	}
	pa_context_set_state_callback(s.context, Impl::stateChanged, &s);
	if (pa_context_connect(s.context, nullptr, PA_CONTEXT_NOAUTOSPAWN, nullptr) < 0 ||
	    pa_threaded_mainloop_start(s.loop) < 0) {
		error = "cannot connect to PipeWire Pulse server";
		return false;
	}
	s.running = true;
	std::unique_lock<std::mutex> lock(s.mutex);
	if (!s.ready.wait_for(lock, std::chrono::seconds(5), [&] {
		    return s.failed || (s.gotDevices && s.gotServer);
	    }) || s.failed) {
		error = "cannot enumerate audio outputs (is pipewire-pulse running?)";
		return false;
	}
	return true;
}

OutputSnapshot OutputMonitor::snapshot() const
{
	std::lock_guard<std::mutex> lock(impl->mutex);
	return impl->data;
}

bool OutputMonitor::setDefault(const std::string &name)
{
	return impl->execute([&](auto *command) {
		return pa_context_set_default_sink(impl->context, name.c_str(),
			Impl::commandDone, command);
	});
}

bool OutputMonitor::setPort(const std::string &name, const std::string &port)
{
	return impl->execute([&](auto *command) {
		return pa_context_set_sink_port_by_name(impl->context, name.c_str(),
			port.c_str(), Impl::commandDone, command);
	});
}

void OutputMonitor::requestDefault(const std::string &name)
{
	pa_threaded_mainloop_lock(impl->loop);
	if (pa_context_get_state(impl->context) == PA_CONTEXT_READY)
		if (auto *op = pa_context_set_default_sink(impl->context, name.c_str(),
				Impl::requestDone, impl.get()))
			pa_operation_unref(op);
	pa_threaded_mainloop_unlock(impl->loop);
}

void OutputMonitor::requestPort(const std::string &name, const std::string &port)
{
	pa_threaded_mainloop_lock(impl->loop);
	if (pa_context_get_state(impl->context) == PA_CONTEXT_READY)
		if (auto *op = pa_context_set_sink_port_by_name(impl->context, name.c_str(),
				port.c_str(), Impl::requestDone, impl.get()))
			pa_operation_unref(op);
	pa_threaded_mainloop_unlock(impl->loop);
}

} // namespace saucefx
