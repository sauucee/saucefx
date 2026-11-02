/*
 * SauceFX - sound EQ and audio enhancement for Linux.
 *
 * The visible interface is the React/TypeScript app in frontend/ (Base UI,
 * Vite), rendered by WebKitGTK through juce_gui_extra. It draws its own
 * title bar and talks to this file over window.saucefxHost; native state,
 * spectrum and toasts come back as saucefxState, saucefxSpectrum and
 * saucefxToast events.
 *
 * The JUCE panel below (SauceFXPanel, FxTheme, FxProView, FxEffects,
 * FxEqualizer) is kept alive as a hidden engine/preset backend. It still
 * uses the original FxSound artwork and the same fxpw wiring as
 * FxController (preset load, effect values, EQ bands, power, routing,
 * spectrum, stats); nothing from it is painted in web mode. This file adds
 * the resource provider, the frameless window's X11 move/shape support, and
 * the minimize/close/quit actions exposed to the web UI.
 *
 * Threading mirrors FxSound: engine control happens on JUCE's message
 * thread, no locks; processAudio runs on the bridge worker.
 */
#include <juce_gui_basics/juce_gui_basics.h>
#include <juce_gui_extra/juce_gui_extra.h>

#if JUCE_LINUX
#include <X11/Xlib.h>
#include <X11/Xatom.h>
#include <X11/extensions/shape.h>
/* Xlib defines these as event-type macros; they would rewrite JUCE names. */
#ifdef KeyPress
#undef KeyPress
#endif
#ifdef KeyRelease
#undef KeyRelease
#endif
#endif

#include <fxpw.h>

#include <wchar.h>
#include <algorithm>
#include <array>
#include <cstddef>
#include <cstring>
#include <memory>
#include <optional>
#include <vector>
#include <cstdio>
#include <cstdlib>
#include <sys/file.h>
#include <sys/stat.h>
#include <unistd.h>
#include <clocale>
#include <cmath>

namespace
{
/* The engine's analyzer exposes exactly this many bands
 * (dfxpDefs.h: DFXP_SPECTRUM_NUM_BANDS; getSpectrumBandValues enforces
 * that size). */
constexpr int DFXP_BAND_COUNT = 10;
constexpr int FX_WEB_WINDOW_RADIUS = 14; /* matches .app-shell */

#if JUCE_LINUX
/* The web interface owns the whole window, including its header. Use the
 * window manager's standard move request so dragging works without a native
 * title bar. This is intentionally Linux-only; SauceFX's desktop app is. */
void beginWindowMove(juce::Component &component)
{
	auto *peer = component.getPeer();
	if (peer == nullptr)
		return;

	auto window = reinterpret_cast<::Window>(peer->getNativeHandle());
	if (window == 0)
		return;

	Display *display = XOpenDisplay(nullptr);
	if (display == nullptr)
		return;

	auto root = DefaultRootWindow(display);
	::Window rootReturn = 0, childReturn = 0;
	int rootX = 0, rootY = 0, windowX = 0, windowY = 0;
	unsigned int mask = 0;
	bool found = XQueryPointer(display, root, &rootReturn, &childReturn,
				   &rootX, &rootY, &windowX, &windowY,
				   &mask) != 0;

	if (found) {
		XEvent event{};
		event.xclient.type = ClientMessage;
		event.xclient.window = window;
		event.xclient.message_type =
			XInternAtom(display, "_NET_WM_MOVERESIZE", False);
		event.xclient.format = 32;
		event.xclient.data.l[0] = rootX;
		event.xclient.data.l[1] = rootY;
		event.xclient.data.l[2] = 8; /* _NET_WM_MOVERESIZE_MOVE */
		event.xclient.data.l[3] = Button1;
		event.xclient.data.l[4] = 1;
		XSendEvent(display, root, False,
			   SubstructureRedirectMask | SubstructureNotifyMask,
			   &event);
		XFlush(display);
	}

	XCloseDisplay(display);
}

/* WebKit paints an opaque view all the way to the window edges. Clip the
 * native X11 window to the same 14 px radius used by the React shell so the
 * desktop still shows through the corners. */
void applyRoundedWindowShape(juce::Component &component)
{
	auto *peer = component.getPeer();
	if (peer == nullptr)
		return;

	auto window = reinterpret_cast<::Window>(peer->getNativeHandle());
	if (window == 0)
		return;

	Display *display = XOpenDisplay(nullptr);
	if (display == nullptr)
		return;

	::Window root = 0;
	int x = 0, y = 0;
	unsigned int width = 0, height = 0, border = 0, depth = 0;
	if (!XGetGeometry(display, window, &root, &x, &y, &width, &height,
			  &border, &depth) ||
	    width == 0 || height == 0) {
		XCloseDisplay(display);
		return;
	}

	double scale = component.getWidth() > 0
			       ? (double)width / component.getWidth()
			       : 1.0;
	int radius = juce::jlimit(
		0, (int)juce::jmin(width, height) / 2,
		(int)std::lround(FX_WEB_WINDOW_RADIUS * scale));
	if (radius <= 0) {
		XCloseDisplay(display);
		return;
	}

	std::vector<XRectangle> rects;
	rects.reserve(height);
	for (unsigned int row = 0; row < height; ++row) {
		int inset = 0;
		double fromTop = radius - (row + 0.5);
		double fromBottom =
			(row + 0.5) - ((double)height - radius);
		if (fromTop > 0.0) {
			inset = radius - (int)std::lround(std::sqrt(
						  (double)radius * radius -
						  fromTop * fromTop));
		} else if (fromBottom > 0.0) {
			inset = radius - (int)std::lround(std::sqrt(
						  (double)radius * radius -
						  fromBottom * fromBottom));
		}
		inset = juce::jlimit(0, (int)width / 2, inset);
		rects.push_back(
			{(short)inset, (short)row,
			 (unsigned short)(width - (unsigned int)inset * 2), 1});
	}

	XShapeCombineRectangles(display, window, ShapeBounding, 0, 0,
				rects.data(), (int)rects.size(), ShapeSet,
				Unsorted);
	XFlush(display);
	XCloseDisplay(display);
}
#else
void beginWindowMove(juce::Component &) {}
void applyRoundedWindowShape(juce::Component &) {}
#endif

/* Locate the installed data tree or a development checkout without relying
 * on the build machine's path. The compile-time path is supplied by CMake
 * for RPM/Flatpak installs; SAUCEFX_ROOT overrides it for development. */
juce::File findRepoRoot()
{
	auto isRepo = [](const juce::File &dir) {
		return dir.getChildFile("assets").getChildFile("Images")
				.isDirectory() &&
		       dir.getChildFile("presets").getChildFile("Factsoft")
				.isDirectory();
	};

	if (const char *env = getenv("SAUCEFX_ROOT"); env != nullptr &&
							env[0] != '\0') {
		juce::File root(juce::String::fromUTF8(env));
		if (isRepo(root))
			return root;
	}

#ifdef SAUCEFX_INSTALL_DATADIR
	{
		juce::File installedRoot(SAUCEFX_INSTALL_DATADIR);
		if (isRepo(installedRoot))
			return installedRoot;
	}
#endif

	auto search = [&](juce::File dir) {
		for (int i = 0; i < 8; ++i) {
			if (isRepo(dir))
				return dir;
			auto parent = dir.getParentDirectory();
			if (parent == dir)
				break;
			dir = parent;
		}
		return juce::File();
	};

	if (auto root = search(juce::File::getCurrentWorkingDirectory());
	    root != juce::File())
		return root;
	if (auto root = search(juce::File::getSpecialLocation(
				      juce::File::currentExecutableFile)
				      .getParentDirectory());
	    root != juce::File())
		return root;
	return {};
}

juce::File repoRoot()
{
	static const auto root = findRepoRoot();
	return root;
}

/* Original app asset directories */
juce::File imagesDir()
{
	return repoRoot().getChildFile("assets").getChildFile("Images");
}

juce::File fontsDir()
{
	return repoRoot().getChildFile("assets").getChildFile("Fonts");
}

/* The CMake build copies frontend/dist next to the executable. The repo
 * fallback makes running the binary directly from the build tree work even
 * before that copy exists, which is handy while iterating on the UI. */
juce::File webRootDir()
{
	auto local = juce::File::getSpecialLocation(
			     juce::File::currentExecutableFile)
			     .getParentDirectory()
			     .getChildFile("web");
	if (local.getChildFile("index.html").existsAsFile())
		return local;

#ifdef SAUCEFX_INSTALL_DATADIR
	{
		auto installed = juce::File(SAUCEFX_INSTALL_DATADIR)
					 .getChildFile("web");
		if (installed.getChildFile("index.html").existsAsFile())
			return installed;
	}
#endif

	auto search = [](juce::File dir) {
		for (int i = 0; i < 8; ++i) {
			auto candidate = dir.getChildFile("frontend").getChildFile(
				"dist");
			if (candidate.getChildFile("index.html").existsAsFile())
				return candidate;
			auto parent = dir.getParentDirectory();
			if (parent == dir)
				break;
			dir = parent;
		}
		return juce::File();
	};

	if (auto root = search(juce::File::getCurrentWorkingDirectory());
	    root != juce::File())
		return root;
	return search(juce::File::getSpecialLocation(
			      juce::File::currentExecutableFile)
			      .getParentDirectory());
}

/* Window/taskbar icon. The SVG is used for the in-app title bar; the
 * raster copy gives the window manager a ready-made image. */
juce::Image appIconImage()
{
	auto file = imagesDir().getChildFile("saucefx-logo-128.png");
	if (!file.existsAsFile())
		return {};
	return juce::ImageFileFormat::loadFrom(file);
}

/* Repository preset library: <repo>/presets/Factsoft */
juce::File presetLibraryDir()
{
	return repoRoot().getChildFile("presets").getChildFile("Factsoft");
}

/* JUCE derives currentExecutableFile from dladdr(), which only reports the
 * name used to start the process when a launcher resolves it through PATH.
 * WebKitGTK re-executes that value in its browser child, so a bare name there
 * kills the web UI unless the working directory happens to contain the
 * binary (the Flatpak launcher is one such case). Resolve /proc/self/exe and
 * switch to its directory when JUCE's path is not usable. */
void ensureResolvableExecutablePath()
{
	auto executable = juce::File::getSpecialLocation(
		juce::File::currentExecutableFile);
	if (executable.existsAsFile())
		return;

	char target[4096];
	auto len = ::readlink("/proc/self/exe", target, sizeof(target) - 1);
	if (len <= 0)
		return;
	target[len] = '\0';

	auto resolved = juce::File(juce::String::fromUTF8(target, (int)len));
	if (!resolved.existsAsFile())
		return;
	if (resolved.getParentDirectory().setAsCurrentWorkingDirectory())
		fprintf(stderr, "saucefx: resolved executable path: %s\n",
			resolved.getFullPathName().toRawUTF8());
}

/* XDG state dir used for the app lock and GUI settings. */
juce::File stateDir()
{
	if (const char *xdg = getenv("XDG_STATE_HOME"); xdg != nullptr &&
							 xdg[0] != '\0')
		return juce::File(juce::String::fromUTF8(xdg))
			.getChildFile("saucefx");
	if (const char *home = getenv("HOME"); home != nullptr &&
					       home[0] != '\0')
		return juce::File(juce::String::fromUTF8(home))
			.getChildFile(".local/state/saucefx");
	return juce::File("/tmp/saucefx");
}

/* XDG data dir: imported (user) presets live here, mirroring the
 * Windows app's %APPDATA%\FxSound\Presets library. */
juce::File dataDir()
{
	if (const char *xdg = getenv("XDG_DATA_HOME"); xdg != nullptr &&
							xdg[0] != '\0')
		return juce::File(juce::String::fromUTF8(xdg))
			.getChildFile("saucefx");
	if (const char *home = getenv("HOME"); home != nullptr &&
					       home[0] != '\0')
		return juce::File(juce::String::fromUTF8(home))
			.getChildFile(".local/share/saucefx");
	return juce::File("/tmp/saucefx");
}

juce::File userPresetDir()
{
	return dataDir().getChildFile("presets");
}

juce::File settingsFile()
{
	return stateDir().getChildFile("settings.txt");
}

/* Live preset session: the selected preset path plus the current engine
 * values. This is intentionally separate from the .fac library so
 * adjusting a factory/user preset never overwrites that file. */
juce::File presetSessionFile()
{
	return stateDir().getChildFile("preset-session.txt");
}

/* A second launch drops this marker so the already-running (possibly
 * hidden) instance can bring its window back without starting a second
 * engine/sink. */
juce::File showRequestFile()
{
	return stateDir().getChildFile("show.request");
}

/* XDG config dir: the login autostart entry lives under autostart/.
 *
 * Flatpak points XDG_CONFIG_HOME into the per-app private config tree, but
 * login autostart entries must be in the host's ~/.config/autostart. The
 * manifest grants access to just that host directory. */
juce::File configDir()
{
#ifdef __linux__
	if (getenv("FLATPAK_ID") != nullptr) {
		if (const char *home = getenv("HOME"); home != nullptr &&
						       home[0] != '\0')
			return juce::File(juce::String::fromUTF8(home))
				.getChildFile(".config");
	}
#endif
	if (const char *xdg = getenv("XDG_CONFIG_HOME"); xdg != nullptr &&
							 xdg[0] != '\0')
		return juce::File(juce::String::fromUTF8(xdg));
	if (const char *home = getenv("HOME"); home != nullptr &&
					       home[0] != '\0')
		return juce::File(juce::String::fromUTF8(home))
			.getChildFile(".config");
	return juce::File("/tmp/saucefx-config");
}

juce::File autostartFile()
{
	return configDir().getChildFile("autostart")
		.getChildFile("saucefx.desktop");
}

/* Create/remove the XDG autostart entry. The entry starts SauceFX with
 * --autostart so login doesn't pop the window over the desktop; the
 * engine runs in the background and a normal launch brings it back. */
bool applyAutostartSetting(bool enabled)
{
	auto file = autostartFile();
	if (!enabled) {
		if (!file.existsAsFile())
			return true;
		bool ok = file.deleteFile();
		if (!ok)
			fprintf(stderr, "saucefx: could not remove %s\n",
				file.getFullPathName().toRawUTF8());
		return ok;
	}

	auto dir = file.getParentDirectory();
	if (!dir.createDirectory())
		return false;

	juce::String execLine;
	juce::String icon;
#ifdef __linux__
	if (const char *flatpakId = getenv("FLATPAK_ID");
	    flatpakId != nullptr && flatpakId[0] != '\0') {
		/* A sandbox path such as /app/bin/SauceFX is not valid on the
		 * host. Ask Flatpak to launch the installed app instead. */
		execLine = "flatpak run " + juce::String::fromUTF8(flatpakId)
			   + " --autostart";
		icon = juce::String::fromUTF8(flatpakId);
	} else
#endif
	{
		auto exe = juce::File::getSpecialLocation(
				   juce::File::currentExecutableFile)
				   .getFullPathName();
		exe = exe.replace("\"", "\\\"").replace("%", "%%");
		execLine = "\"" + exe + "\" --autostart";
		icon = imagesDir()
			       .getChildFile("saucefx-logo-512.png")
			       .getFullPathName();
		icon = icon.replace("\"", "\\\"");
	}

	juce::String text;
	text << "[Desktop Entry]\n"
		"Type=Application\n"
		"Version=1.0\n"
		"Name=SauceFX\n"
		"Comment=SauceFX sound EQ and audio enhancement for Linux\n"
		"Icon=\"" << icon << "\"\n"
		"Exec=" << execLine << "\n"
		"Terminal=false\n"
		"X-GNOME-Autostart-enabled=true\n"
		"X-GNOME-Autostart-Delay=2\n";

	/* Desktop entries must use LF, not the Windows CRLF default of
	 * File::replaceWithText. */
	bool ok = file.replaceWithText(text, false, false, "\n");
	if (!ok)
		fprintf(stderr, "saucefx: could not write %s\n",
			file.getFullPathName().toRawUTF8());
	return ok;
}

static juce::Array<juce::File> findPresetsIn(const juce::File &dir)
{
	juce::Array<juce::File> out;
	if (dir.isDirectory())
		out = dir.findChildFiles(juce::File::findFiles, false, "*.fac");

	if (out.size() > 1)
		std::sort(out.begin(), out.end(),
			  [](const juce::File &a, const juce::File &b) {
				  return a.getFileName().compareIgnoreCase(
						 b.getFileName()) < 0;
			  });
	return out;
}

juce::Array<juce::File> findPresets()
{
	/* factory library first, then the user's imported presets */
	auto out = findPresetsIn(presetLibraryDir());
	out.addArray(findPresetsIn(userPresetDir()));
	return out;
}

/* ---- persisted GUI settings (theme + custom accent + startup) ---- */

static juce::String fx_output_device_;
static juce::String fx_output_port_;

struct GuiSettings {
	int themeMode = 0; /* 0 = dark, 1 = light */
	bool customAccent = false;
	uint32_t accent = 0xffe63462u;
	bool autostart = false; /* new installs opt in from Settings */
	bool outputRouted = false; /* restore last output choice */
	juce::String outputDevice;
	juce::String outputPort;
};

GuiSettings loadGuiSettings()
{
	GuiSettings s;
	auto f = settingsFile();
	if (!f.existsAsFile())
		return s;

	for (auto line : juce::StringArray::fromLines(f.loadFileAsString())) {
		line = line.trim();
		if (line.startsWith("theme=")) {
			auto v = line.fromFirstOccurrenceOf("=", false, false)
					 .trim();
			s.themeMode = v.equalsIgnoreCase("light") ? 1 : 0;
		} else if (line.startsWith("accent=")) {
			auto v = line.fromFirstOccurrenceOf("=", false, false)
					 .trim();
			if (v.equalsIgnoreCase("default") || v.isEmpty()) {
				s.customAccent = false;
			} else {
				if (v.startsWith("#"))
					v = v.substring(1);
				if (v.length() == 6) {
					s.accent = 0xff000000u |
						   ((uint32_t)v.getHexValue32() &
						    0xffffffu);
					s.customAccent = true;
				}
			}
		} else if (line.startsWith("autostart=")) {
			auto v = line.fromFirstOccurrenceOf("=", false, false)
					 .trim();
			s.autostart = !v.equalsIgnoreCase("off");
		} else if (line.startsWith("output=")) {
			auto v = line.fromFirstOccurrenceOf("=", false, false)
					 .trim();
			s.outputRouted = v.equalsIgnoreCase("saucefx");
		} else if (line.startsWith("output-device=")) {
			s.outputDevice = line.fromFirstOccurrenceOf("=", false, false);
		} else if (line.startsWith("output-port=")) {
			s.outputPort = line.fromFirstOccurrenceOf("=", false, false);
		}
	}
	return s;
}

void saveGuiSettings(int themeMode, bool customAccent, uint32_t accent,
		     bool autostart, bool outputRouted)
{
	auto dir = stateDir();
	dir.createDirectory();
	juce::String text;
	text << "theme=" << (themeMode == 1 ? "light" : "dark") << "\n";
	if (customAccent)
		text << "accent=#"
		     << juce::String::toHexString((int)(accent & 0xffffffu))
				.paddedLeft('0', 6)
		     << "\n";
	else
		text << "accent=default\n";
	text << "autostart=" << (autostart ? "on" : "off") << "\n";
	text << "output=" << (outputRouted ? "saucefx" : "system")
	     << "\n";
	text << "output-device=" << fx_output_device_ << "\n"
	     << "output-port=" << fx_output_port_ << "\n";
	settingsFile().replaceWithText(text, false, false, "\n");
}

/* The values captured by the live session. The preset path identifies
 * the dropdown selection; the value arrays preserve unsaved effect/EQ
 * tweaks so a full quit does not throw them away. */
struct PresetSession {
	bool valid = false;
	juce::String presetPath;
	bool power = true;
	std::array<float, 5> effects{};
	int eqBands = 0;
	std::array<float, 32> eqFreq{};
	std::array<float, 32> eqGain{};
};

PresetSession loadPresetSession()
{
	PresetSession s;
	auto f = presetSessionFile();
	if (!f.existsAsFile())
		return s;

	int version = 0;
	int effectsSeen = 0;
	bool effectSeen[5] = {};
	int eqFreqSeen = 0;
	int eqGainSeen = 0;

	for (auto line :
	     juce::StringArray::fromLines(f.loadFileAsString())) {
		line = line.trim();
		if (line.isEmpty())
			continue;

		if (line.startsWith("version=")) {
			version = line.fromFirstOccurrenceOf("=", false, false)
					  .getIntValue();
		} else if (line.startsWith("preset=")) {
			s.presetPath =
				line.fromFirstOccurrenceOf("=", false, false)
					.trim();
		} else if (line.startsWith("power=")) {
			auto v = line.fromFirstOccurrenceOf("=", false, false)
					 .trim();
			s.power = !v.equalsIgnoreCase("off") &&
				  v != "0";
		} else if (line.startsWith("bands=")) {
			s.eqBands =
				line.fromFirstOccurrenceOf("=", false, false)
					.getIntValue();
		} else if (line.startsWith("effect")) {
			auto key = line.upToFirstOccurrenceOf("=", false, false);
			int i = key.substring(6).getIntValue();
			float v = line.fromFirstOccurrenceOf("=", false, false)
					  .getFloatValue();
			if (i >= 0 && i < 5 && std::isfinite(v)) {
				s.effects[i] = v;
				if (!effectSeen[i]) {
					effectSeen[i] = true;
					effectsSeen++;
				}
			}
		} else if (line.startsWith("eq")) {
			auto key = line.upToFirstOccurrenceOf("=", false, false);
			int i = key.substring(2).getIntValue();
			float v = line.fromFirstOccurrenceOf("=", false, false)
					  .getFloatValue();
			if (i >= 0 && i < 32 && std::isfinite(v)) {
				if (key.contains("freq")) {
					s.eqFreq[i] = v;
					eqFreqSeen++;
				} else if (key.contains("gain")) {
					s.eqGain[i] = v;
					eqGainSeen++;
				}
			}
		}
	}

	if (version == 1 && effectsSeen == 5 && s.eqBands >= 1 &&
	    s.eqBands <= 32 && eqFreqSeen >= s.eqBands &&
	    eqGainSeen >= s.eqBands)
		s.valid = true;

	return s;
}

juce::FontOptions optionsWith(const juce::Typeface::Ptr &tf, float h)
{
	auto o = juce::FontOptions().withHeight(h);
	return tf ? o.withTypeface(tf) : o;
}
} // namespace

/* ================================================================== */
/* FxTheme tables: the dark + light color tables, Figtree faces, SVGs */

enum FxColor : int {
	WindowBackground, WidgetBackground, MenuBackground, Outline,
	DefaultText, DefaultFill, HighlightedText, HighlightedFill, MenuText,
	ComboBoxBackground, TextButtonBackground, ImageButton, HintText,
	ValidTextBorder, InvalidTextBorder, ControlBackground, SliderTrack,
	SliderHighlight, GraphHigh, GraphLow, EqStart, EqEnd,
	VerticalSliderLow, MenuHighlightBackground, PanelBackground,
	RowOutline, SelectedRowOutline, NumColors
};

enum FxThemeMode : int { ThemeDark = 0, ThemeLight, NumThemeModes };

/* FxTheme::theme_colors_ - the dark row and a softened, warm light row.
 * The upstream light mode used near-white surfaces; this version keeps
 * the same contrast relationships with a paper-grey background. */
static const uint32_t fx_theme_colors_[NumThemeModes][NumColors] = {
	{ 0x181818, 0x181818, 0x383838, 0x2b2b2b, 0xb1b1b1, 0x000000,
	  0xffffff, 0x0c0c0c, 0xffffff, 0x000000, 0xd51535, 0xe63462,
	  0x7f7f7f, 0x009cdd, 0xd51535, 0x0f0f0f, 0xe33250, 0xf7546f,
	  0xd51535, 0xfe566a, 0xef4b65, 0x742834, 0xf3f3f3, 0x414141,
	  0x000000, 0xb1b1b1, 0xe63462 },
	{ 0xe7e3de, 0xe7e3de, 0xd4cfc9, 0xc9c3bb, 0x3b3834, 0xf3efea,
	  0x272522, 0xd9d3cc, 0x3b3834, 0xd8d2cb, 0x1ac1ff, 0x23b6eb,
	  0x625c56, 0x0087b5, 0xc33b4d, 0xddd8d2, 0x0a4d66, 0x53ccff,
	  0x1ac1ff, 0x72d8ff, 0x33c8ff, 0x063244, 0x3b3834, 0xc9c3bb,
	  0x3b3834, 0xc2bbb2, 0x3b3834 }
};

static int fx_theme_mode_ = ThemeDark;
static bool fx_accent_custom_ = false;
static uint32_t fx_accent_argb_ = 0xffe63462u;
static bool fx_autostart_enabled_ = false;
static bool fx_output_routed_ = false;

static std::array<uint32_t, NumColors> fx_active_colors_ = [] {
	std::array<uint32_t, NumColors> a{};
	for (int i = 0; i < NumColors; i++)
		a[(size_t)i] = fx_theme_colors_[ThemeDark][i];
	return a;
}();

static inline uint32_t fxRgb(juce::Colour c)
{
	return c.getARGB() & 0xffffffu;
}

/* Base dark/light palette plus the optional user accent override.
 * The accent index set mirrors the upstream reds/blues (selection,
 * graph, EQ curve, highlighted icon accents). */
static void fx_apply_palette(int mode, bool customAccent, uint32_t accent)
{
	if (mode < 0 || mode >= NumThemeModes)
		mode = ThemeDark;
	fx_theme_mode_ = mode;
	fx_accent_custom_ = customAccent;
	fx_accent_argb_ = 0xff000000u | (accent & 0xffffffu);

	for (int i = 0; i < NumColors; i++)
		fx_active_colors_[(size_t)i] = fx_theme_colors_[mode][i];

	if (customAccent) {
		juce::Colour c(fx_accent_argb_);
		fx_active_colors_[ImageButton] = fxRgb(c);
		fx_active_colors_[SliderTrack] = fxRgb(c);
		fx_active_colors_[SliderHighlight] = fxRgb(c.brighter(0.18f));
		fx_active_colors_[GraphHigh] = fxRgb(c);
		fx_active_colors_[GraphLow] = fxRgb(c.brighter(0.35f));
		fx_active_colors_[EqStart] = fxRgb(c.brighter(0.12f));
		fx_active_colors_[EqEnd] = fxRgb(c.darker(0.55f));
		fx_active_colors_[VerticalSliderLow] = fxRgb(
			mode == ThemeDark ? c.brighter(0.55f)
					  : c.darker(0.55f));
		fx_active_colors_[SelectedRowOutline] = fxRgb(c);
		fx_active_colors_[ValidTextBorder] = fxRgb(c);
	}
}

static inline juce::Colour fxc(int id, float a = 1.0f)
{
	return juce::Colour(0xff000000u | fx_active_colors_[(size_t)id])
		.withAlpha(a);
}

/* Strong ease-out, matching the UI motion in the design reference. */
static float fxEaseOutCubic(float t)
{
	t = juce::jlimit(0.0f, 1.0f, t);
	float inv = 1.0f - t;
	return 1.0f - inv * inv * inv;
}

static juce::AffineTransform fxScaleAround(float scale,
					   juce::Point<float> centre)
{
	return juce::AffineTransform::scale(scale, scale, centre.x,
					    centre.y);
}

/* FxTheme constants */
constexpr int FX_TITLE_BAR_HEIGHT = 57;
constexpr int FX_WINDOW_CORNER_RADIUS = 21; /* FxTheme::WINDOW_CORNER_RADIUS */
constexpr int FX_SLIDER_THUMB_RADIUS = 8;
constexpr int FX_ROTARY_SLIDER_THUMB_RADIUS = 5;

class FxTheme
{
public:
	static FxTheme &get()
	{
		static FxTheme inst;
		return inst;
	}

	/* Figtree 400/600/700: OFL-licensed UI faces */
	juce::Typeface::Ptr regular;
	juce::Typeface::Ptr semibold;
	juce::Typeface::Ptr bold;

	/* the per-mode drawables FxTheme::init() builds */
	std::unique_ptr<juce::Drawable> dropDownArrow;    /* dropdown_arrow_hover */
	std::unique_ptr<juce::Drawable> dropDownArrowGrey; /* dropdown_arrow_bw  */
	std::unique_ptr<juce::Drawable> sliderThumb;     /* Slider_Thumb        */
	std::unique_ptr<juce::Drawable> sliderThumbGrey; /* Slider_Thumb_bw     */
	std::unique_ptr<juce::Drawable> powerOn;
	std::unique_ptr<juce::Drawable> powerOff;
	std::unique_ptr<juce::Drawable> logo;
	std::unique_ptr<juce::Drawable> restoreDefaults;
	std::unique_ptr<juce::Drawable> settingsGear;
	std::unique_ptr<juce::Drawable> settingsGearActive;

	/* apply a base mode + optional accent live (called from the GUI) */
	static void setTheme(int mode, bool customAccent, uint32_t accent)
	{
		fx_apply_palette(mode, customAccent, accent);
		get().loadDrawables();
	}

	juce::Font normalFont(float h) const  { return { optionsWith(semibold, h) }; }
	juce::Font smallFont(float h) const   { return { optionsWith(regular, h) }; }
	juce::Font titleFont(float h) const   { return { optionsWith(bold, h) }; }

	void loadDrawables()
	{
		const bool light = fx_theme_mode_ == ThemeLight;

		dropDownArrow = svg(light ? "dropdown_arrow_hover_blue.svg"
					  : "dropdown_arrow_hover.svg");
		dropDownArrowGrey = svg("dropdown_arrow_bw.svg");
		sliderThumb = svg(light ? "Slider_Thumb_blue.svg"
					: "Slider_Thumb.svg");
		sliderThumbGrey = svg("Slider_Thumb_bw.svg");
		powerOn = svg(light ? "power_on_blue.svg" : "power_on.svg");
		powerOff = svg(light ? "power_off_black.svg"
				     : "power_off.svg");
		logo = svg("saucefx-mark.svg");
		restoreDefaults = svg(light ? "restore_defaults_blue.svg"
					    : "restore_defaults_white.svg");

		settingsGear = svg("settings.svg");
		if (settingsGear)
			settingsGear->replaceColour(
				juce::Colour(0xff7e7e7e), fxc(DefaultText));
		settingsGearActive = svg("settings.svg");
		if (settingsGearActive)
			settingsGearActive->replaceColour(
				juce::Colour(0xff7e7e7e), fxc(ImageButton));

		/* the original power/dropdown SVGs hard-code the accent
		 * (pink in dark, blue in light); retint them when the
		 * user picks a custom accent */
		if (fx_accent_custom_) {
			juce::Colour src(light ? 0xff23b6eb : 0xffe63462);
			juce::Colour dst(fx_accent_argb_);
			if (powerOn)
				powerOn->replaceColour(src, dst);
			if (dropDownArrow)
				dropDownArrow->replaceColour(src, dst);
		}
	}

private:
	static std::unique_ptr<juce::Drawable> svg(const char *name)
	{
		auto f = imagesDir().getChildFile(juce::String(name));
		if (!f.existsAsFile())
			return nullptr;
		juce::MemoryBlock block;
		if (!f.loadFileAsData(block))
			return nullptr;
		return juce::Drawable::createFromImageData(block.getData(),
							  block.getSize());
	}

	static juce::Typeface::Ptr ttf(const char *name)
	{
		auto f = fontsDir().getChildFile(juce::String(name));
		if (!f.existsAsFile())
			return nullptr;
		juce::MemoryBlock block;
		if (!f.loadFileAsData(block))
			return nullptr;
		return juce::Typeface::createSystemTypefaceFor(block.getData(),
							      block.getSize());
	}

	FxTheme()
	{
		regular = ttf("Figtree-Regular.ttf");
		semibold = ttf("Figtree-SemiBold.ttf");
		bold = ttf("Figtree-Bold.ttf");

		loadDrawables();
	}
};

static void drawWithinIf(juce::Graphics &g,
			 const std::unique_ptr<juce::Drawable> &d,
			 const juce::Rectangle<float> &where)
{
	if (d)
		d->drawWithin(g, where, juce::RectanglePlacement::centred,
			      1.0f);
}

/* ================================================================== */
/* Slider that only reacts when the pointer starts on (or within a few
 * pixels of) the thumb - stray clicks elsewhere in a track don't yank
 * the value (JUCE default behavior). Kept from the earlier implementation: the
 * Windows GUI effectively allows thumb-only dragging. */

class DragOnlySlider : public juce::Slider
{
public:
	bool nearThumb(const juce::MouseEvent &e)
	{
		double range = getMaximum() - getMinimum();
		if (range <= 0.0)
			return true;

		/* Compare against the real thumb centre. getPositionOfValue()
		 * uses the LookAndFeel's slider layout (our horizontal
		 * layout reserves 32 px on the right for the readout), so
		 * a normalized value comparison rejects the thumb at high
		 * values and makes those sliders impossible to grab. */
		bool vertical = getSliderStyle() ==
				juce::Slider::LinearVertical;
		double thumbPos = getPositionOfValue(getValue());
		double mousePos = vertical ? (double)e.position.y
					   : (double)e.position.x;
		return std::abs(mousePos - thumbPos) <= 26.0;
	}

	void mouseDown(const juce::MouseEvent &e) override
	{
		engaged_ = nearThumb(e);
		if (engaged_)
			juce::Slider::mouseDown(e);
	}

	void mouseDrag(const juce::MouseEvent &e) override
	{
		if (engaged_)
			juce::Slider::mouseDrag(e);
	}

	void mouseUp(const juce::MouseEvent &e) override
	{
		if (engaged_) {
			engaged_ = false;
			juce::Slider::mouseUp(e);
		}
	}

protected:
	bool engaged_ = false;
};

/* Shared button feedback: a quick, eased press scale. Hover feedback
 * stays in the paint code (color only) so moving the pointer across
 * the title bar is never animated or delayed. */
class FxPressButton : public juce::Button, private juce::Timer
{
public:
	explicit FxPressButton(const juce::String &name = {})
		: juce::Button(name)
	{
	}

	float pressScale(float minScale = 0.96f) const
	{
		return 1.0f - (1.0f - minScale) * pressAmount_;
	}

protected:
	void mouseDown(const juce::MouseEvent &e) override
	{
		targetPress_ = 1.0f;
		startTimerHz(60);
		juce::Button::mouseDown(e);
	}

	void mouseUp(const juce::MouseEvent &e) override
	{
		targetPress_ = 0.0f;
		startTimerHz(60);
		juce::Button::mouseUp(e);
	}

	void mouseExit(const juce::MouseEvent &e) override
	{
		if (!isMouseButtonDown() && targetPress_ != 0.0f) {
			targetPress_ = 0.0f;
			startTimerHz(60);
		}
		juce::Button::mouseExit(e);
	}

private:
	void timerCallback() override
	{
		/* ~100 ms in, ~75 ms out: the press feels immediate, the
		 * release gets out of the way quickly. */
		if (pressAmount_ < targetPress_)
			pressAmount_ = juce::jmin(
				targetPress_, pressAmount_ + 0.16f);
		else
			pressAmount_ = juce::jmax(
				targetPress_, pressAmount_ - 0.22f);

		repaint();

		if (std::abs(pressAmount_ - targetPress_) < 0.001f) {
			pressAmount_ = targetPress_;
			stopTimer();
		}
	}

	float pressAmount_ = 0.0f;
	float targetPress_ = 0.0f;
};

/* ================================================================== */
/* FxLookAndFeel - SauceFX implementation of the FxTheme drawing code   */

class FxLookAndFeel : public juce::LookAndFeel_V4
{
public:
	FxLookAndFeel() { applyTheme(); }

	/* re-apply the FxTheme colour wiring after a live theme change */
	void applyTheme()
	{
		/* getFxColourScheme() */
		setColourScheme({ fxc(WindowBackground), fxc(WidgetBackground),
			fxc(MenuBackground), fxc(Outline), fxc(DefaultText),
			/* FxTheme quirk: DefaultFill at 0.2 alpha */
			fxc(DefaultFill, 0.2f),
			fxc(HighlightedText), fxc(HighlightedFill),
			fxc(MenuText) });

		/* FxTheme::init() colour wiring */
		setColour(juce::ComboBox::arrowColourId, fxc(ImageButton));
		setColour(juce::ComboBox::backgroundColourId,
			  fxc(ComboBoxBackground));
		setColour(juce::ComboBox::outlineColourId,
			  fxc(ComboBoxBackground));
		setColour(juce::ComboBox::focusedOutlineColourId,
			  fxc(SliderHighlight, 0.2f));
		setColour(juce::ComboBox::textColourId, fxc(DefaultText));
		setColour(juce::TextEditor::backgroundColourId, fxc(DefaultFill));
		setColour(juce::TextEditor::outlineColourId, fxc(DefaultFill));
		setColour(juce::TextEditor::focusedOutlineColourId,
			  fxc(DefaultFill));
		setColour(juce::TextEditor::textColourId, fxc(DefaultText));
		setColour(juce::TextEditor::highlightedTextColourId,
			  fxc(HighlightedText));
		setColour(juce::TextButton::buttonColourId,
			  fxc(TextButtonBackground));
		setColour(juce::TextButton::buttonOnColourId,
			  fxc(TextButtonBackground));
		setColour(juce::TextButton::textColourOffId, fxc(HighlightedText));
		setColour(juce::TextButton::textColourOnId, fxc(HighlightedText));
		setColour(juce::CaretComponent::caretColourId, fxc(DefaultText));
		setColour(juce::PopupMenu::backgroundColourId, fxc(DefaultFill));
		setColour(juce::PopupMenu::highlightedBackgroundColourId,
			  fxc(ImageButton));
		setColour(juce::PopupMenu::textColourId, fxc(DefaultText));
		setColour(juce::PopupMenu::highlightedTextColourId,
			  fxc(HighlightedText));
		setColour(juce::Label::textColourId, fxc(DefaultText));
		setColour(juce::Slider::rotarySliderOutlineColourId,
			  fxc(SliderTrack, 0.2f));
		setColour(juce::Slider::rotarySliderFillColourId, fxc(SliderTrack));
		setColour(juce::ScrollBar::thumbColourId, fxc(SliderTrack));
		setColour(juce::ResizableWindow::backgroundColourId,
			  fxc(WindowBackground));

		auto &t = FxTheme::get();
		if (t.semibold)
			setDefaultSansSerifTypeface(t.semibold);
	}

	juce::Font fxFont600(float h) const
	{
		return { optionsWith(FxTheme::get().semibold, h) };
	}

	/* ---- combo boxes (FxTheme::drawComboBox et al.) ---- */
	juce::Font getComboBoxFont(juce::ComboBox &box) override
	{
		return box.getHeight() <= 30 ? fxFont600(14.0f)
					     : fxFont600(17.0f);
	}

	void positionComboBoxText(juce::ComboBox &box,
				  juce::Label &label) override
	{
		label.setMinimumHorizontalScale(1.0);
		LookAndFeel_V4::positionComboBoxText(box, label);
		label.setBounds(label.getBounds()
					.withX(5)
					.withRight(box.getWidth() - 37));
	}

	void drawComboBox(juce::Graphics &g, int width, int height, bool,
			  int, int, int, int, juce::ComboBox &box) override
	{
		auto cornerSize = (float)height / 5;
		juce::Rectangle<int> boxBounds(0, 0, width, height);

		g.setColour(box.findColour(juce::ComboBox::backgroundColourId));
		g.fillRoundedRectangle(boxBounds.toFloat(), cornerSize);

		if (box.hasKeyboardFocus(true)) {
			g.setColour(box.findColour(
				juce::ComboBox::focusedOutlineColourId));
			g.drawRoundedRectangle(
				boxBounds.toFloat().reduced(0.5f, 0.5f),
				cornerSize, 1.0f);
		} else {
			g.setColour(box.findColour(
				juce::ComboBox::outlineColourId));
			g.drawRoundedRectangle(
				boxBounds.toFloat().reduced(0.5f, 0.5f),
				cornerSize, 1.0f);
		}

		int margin = 32;
		if (width <= 150)
			margin = 24;

		auto &t = FxTheme::get();
		auto arrowArea = juce::Rectangle<float>(width - margin, 0, 12,
							height);
		if (box.isEnabled())
			drawWithinIf(g, t.dropDownArrow, arrowArea);
		else
			drawWithinIf(g, t.dropDownArrowGrey, arrowArea);
	}

	void drawComboBoxTextWhenNothingSelected(juce::Graphics &g,
						 juce::ComboBox &box,
						 juce::Label &label) override
	{
		g.setColour(box.findColour(juce::ComboBox::textColourId)
				    .withMultipliedAlpha(0.5f));
		g.setFont(label.getLookAndFeel().getLabelFont(label));

		auto textArea = getLabelBorderSize(label).subtractedFrom(
			label.getLocalBounds().withX(10));

		g.drawFittedText(box.getTextWhenNothingSelected(), textArea,
				 label.getJustificationType(),
				 juce::jmax(1, (int)((float)textArea.getHeight() /
						     label.getFont().getHeight())),
				 label.getMinimumHorizontalScale());
	}

	void drawSliderThumb(juce::Graphics &g, juce::Rectangle<float> area,
			     bool enabled)
	{
		if (!enabled) {
			drawWithinIf(g, FxTheme::get().sliderThumbGrey, area);
			return;
		}
		if (!fx_accent_custom_) {
			drawWithinIf(g, FxTheme::get().sliderThumb, area);
			return;
		}

		/* The shipped thumb SVGs hard-code the red/blue accent.
		 * Draw the same shape with the user accent instead. */
		auto accent = juce::Colour(0xff000000u |
					   fx_active_colors_[ImageButton]);

		juce::Path shadow;
		shadow.addEllipse(area.translated(0.0f, 1.2f));
		juce::DropShadow(accent.withAlpha(0.35f), 3, { 0, 1 })
			.drawForPath(g, shadow);

		juce::ColourGradient grad(
			accent.brighter(0.10f), area.getTopLeft(),
			accent.darker(0.30f), area.getBottomRight(), false);
		g.setGradientFill(grad);
		g.fillEllipse(area);

		g.setColour(accent.darker(0.55f));
		g.drawEllipse(area.reduced(0.5f), 1.0f);

		auto dot = area.withSizeKeepingCentre(
			area.getWidth() * 0.375f,
			area.getHeight() * 0.375f);
		g.setColour(accent.getPerceivedBrightness() < 0.4f
				    ? juce::Colours::white.withAlpha(0.85f)
				    : juce::Colour(0xff0f0f0f));
		g.fillEllipse(dot);
	}

	/* ---- sliders (FxTheme::drawLinearSlider / drawRotarySlider /
	 * getSliderLayout, verbatim geometry) ---- */
	void drawLinearSlider(juce::Graphics &g, int x, int y, int width,
			      int height, float sliderPos, float minSliderPos,
			      float maxSliderPos,
			      juce::Slider::SliderStyle style,
			      juce::Slider &slider) override
	{
		(void)minSliderPos;
		(void)maxSliderPos;

		float radius = (float)getSliderThumbRadius(slider);

		if (style == juce::Slider::LinearVertical) {
			float dash_lengths[] = { 5, 2 };

			juce::Colour colour1 = fxc(SliderTrack, 0.4f);
			juce::Colour colour2 = fxc(VerticalSliderLow, 0.4f);
			if (!slider.isEnabled()) {
				colour1 = colour1.withSaturation(0.0f);
				colour2 = colour2.withSaturation(0.0f);
			}

			g.setGradientFill(juce::ColourGradient(
				colour1, juce::Point<float>(0, 0), colour2,
				juce::Point<float>(0, (float)height), false));
			g.drawDashedLine(
				juce::Line<float>(
					(float)x + (float)width / 2, (float)y,
					(float)x + (float)width / 2,
					(float)y + (float)height),
				dash_lengths, 2, 1.0f);

			if (slider.isEnabled())
				drawSliderThumb(
					g,
					juce::Rectangle<float>(
						(float)x + (float)width / 2 - radius,
						sliderPos - radius,
						radius * 2, radius * 2),
					true);
			else
				drawSliderThumb(
					g,
					juce::Rectangle<float>(
						(float)x + (float)width / 2 - radius,
						sliderPos - radius,
						radius * 2, radius * 2),
					false);

			if (slider.getThumbBeingDragged() >= 0 ||
			    slider.hasKeyboardFocus(true)) {
				g.setFillType(fxc(SliderHighlight, 0.1f));
				g.fillRoundedRectangle(
					juce::Rectangle<float>(
						(float)x + (float)(width - FX_SLIDER_THUMB_RADIUS * 4) / 2,
						(float)y,
						(float)(FX_SLIDER_THUMB_RADIUS * 4),
						(float)height)
						.expanded(0, FX_SLIDER_THUMB_RADIUS),
					20);
			}
		} else if (style == juce::Slider::LinearHorizontal) {
			juce::Colour colour = fxc(SliderTrack, 0.2f);
			if (!slider.isEnabled())
				colour = colour.withSaturation(0.0f);

			g.setFillType(colour);
			g.fillRoundedRectangle((float)x,
					       (float)y + (height - 3) / 2,
					       (float)width, 3, 5.6f);

			colour = fxc(SliderTrack);
			if (!slider.isEnabled())
				colour = colour.withSaturation(0.0f);

			g.setFillType(colour);
			g.fillRoundedRectangle(
				(float)x, (float)y + (height - 3) / 2,
				sliderPos, 3, 5.6f);

			if (slider.isEnabled())
				drawSliderThumb(
					g,
					juce::Rectangle<float>(
						sliderPos - radius,
						(float)y + (float)height / 2 - radius,
						radius * 2, radius * 2),
					true);
			else
				drawSliderThumb(
					g,
					juce::Rectangle<float>(
						sliderPos - radius,
						(float)y + (float)height / 2 - radius,
						radius * 2, radius * 2),
					false);

			if (slider.hasKeyboardFocus(true)) {
				g.setFillType(fxc(SliderHighlight, 0.1f));
				g.fillRoundedRectangle(
					juce::Rectangle<float>(
						(float)x, (float)y, (float)width,
						(float)height)
						.expanded(
							FX_SLIDER_THUMB_RADIUS / 2,
							FX_SLIDER_THUMB_RADIUS / 2),
					(float)(height + FX_SLIDER_THUMB_RADIUS));
			}
		} else {
			LookAndFeel_V4::drawLinearSlider(g, x, y, width, height,
							 sliderPos, minSliderPos,
							 maxSliderPos, style,
							 slider);
		}
	}

	void drawRotarySlider(juce::Graphics &g, int x, int y, int width,
			      int height, float sliderPos,
			      float rotaryStartAngle, float rotaryEndAngle,
			      juce::Slider &slider) override
	{
		auto outline = slider.findColour(
			juce::Slider::rotarySliderOutlineColourId);
		auto fill = slider.findColour(
			juce::Slider::rotarySliderFillColourId);

		if (!slider.isEnabled()) {
			outline = outline.withSaturation(0.0f);
			fill = fill.withSaturation(0.0f);
		}

		auto bounds = juce::Rectangle<int>(x, y, width, height)
				      .toFloat()
				      .reduced(2);

		auto radius =
			juce::jmin(bounds.getWidth(), bounds.getHeight()) / 2.0f;
		auto toAngle = rotaryStartAngle +
			       sliderPos * (rotaryEndAngle - rotaryStartAngle);
		auto lineW = 5.0f;
		auto arcRadius = radius - lineW * 0.5f;

		juce::Path backgroundArc;
		backgroundArc.addCentredArc(bounds.getCentreX(),
					    bounds.getCentreY(), arcRadius,
					    arcRadius, 0.0f, rotaryStartAngle,
					    rotaryEndAngle, true);

		g.setColour(outline);
		g.strokePath(backgroundArc,
			     juce::PathStrokeType(
				     lineW, juce::PathStrokeType::curved,
				     juce::PathStrokeType::rounded));

		juce::Path valueArc;
		valueArc.addCentredArc(bounds.getCentreX(), bounds.getCentreY(),
				       arcRadius, arcRadius, 0.0f,
				       rotaryStartAngle, toAngle, true);
		g.setColour(fill);
		g.strokePath(valueArc,
			     juce::PathStrokeType(
				     lineW, juce::PathStrokeType::curved,
				     juce::PathStrokeType::rounded));

		if (slider.hasKeyboardFocus(true)) {
			juce::DropShadow shadow;
			shadow.colour = fxc(SliderHighlight, 0.1f);
			shadow.drawForPath(g, backgroundArc);
		}

		auto thumbRadius = (float)getSliderThumbRadius(slider);
		juce::Point<float> thumbPoint(
			bounds.getCentreX() +
				arcRadius * std::cos(
					toAngle - juce::MathConstants<float>::halfPi),
			bounds.getCentreY() +
				arcRadius * std::sin(
					toAngle - juce::MathConstants<float>::halfPi));

		auto thumbArea =
			juce::Rectangle<float>(thumbPoint.getX() - thumbRadius,
					       thumbPoint.getY() - thumbRadius,
					       thumbRadius * 2, thumbRadius * 2);
		drawSliderThumb(g, thumbArea, slider.isEnabled());
	}

	int getSliderThumbRadius(juce::Slider &slider) override
	{
		return slider.getSliderStyle() == juce::Slider::Rotary
			       ? FX_ROTARY_SLIDER_THUMB_RADIUS
			       : FX_SLIDER_THUMB_RADIUS;
	}

	juce::Slider::SliderLayout getSliderLayout(juce::Slider &slider) override
	{
		auto layout = LookAndFeel_V4::getSliderLayout(slider);
		auto style = slider.getSliderStyle();

		if (style == juce::Slider::LinearVertical) {
			auto yy = layout.sliderBounds.getY() +
				  (FX_SLIDER_THUMB_RADIUS * 2);
			auto hh = layout.sliderBounds.getHeight() -
				  (FX_SLIDER_THUMB_RADIUS * 2);
			layout.sliderBounds.setY(yy);
			layout.sliderBounds.setHeight(hh);
		} else if (style == juce::Slider::LinearHorizontal) {
			auto ww = layout.sliderBounds.getWidth() -
				  (FX_SLIDER_THUMB_RADIUS * 4);
			layout.sliderBounds.setWidth(ww);
		}

		return layout;
	}

	/* ---- popup menu ---- */
	void drawPopupMenuItem(juce::Graphics &g,
			       const juce::Rectangle<int> &area,
			       bool is_separator, bool is_active,
			       bool is_highlighted, bool is_ticked,
			       bool has_submenu, const juce::String &text,
			       const juce::String &shortcut_key_text,
			       const juce::Drawable *icon,
			       const juce::Colour *text_colour) override
	{
		LookAndFeel_V4::drawPopupMenuItem(
			g, area, is_separator, is_active,
			is_highlighted || is_ticked, is_ticked, has_submenu,
			text, shortcut_key_text, icon, text_colour);

		if (is_ticked) {
			g.setColour(fxc(DefaultText));
			g.drawRect(area.toFloat());
		}
	}

	juce::Font getPopupMenuFont() override
	{
		return fxFont600(17.0f);
	}

	juce::Font getTextButtonFont(juce::TextButton &, int buttonHeight) override
	{
		return fxFont600(juce::jmin(17.0f, (float)buttonHeight));
	}

	void drawLabel(juce::Graphics &g, juce::Label &l) override
	{
		g.setColour(l.findColour(juce::Label::textColourId));
		g.setFont(l.getFont());
		g.drawText(l.getText(), l.getLocalBounds(),
			   l.getJustificationType(), true);
	}

	/* ---- document window title bar (FxTheme styling, rounded top) ---- */
	void drawDocumentWindowTitleBar(juce::DocumentWindow &window,
					juce::Graphics &g, int w, int h,
					int titleSpaceX1, int titleSpaceW,
					const juce::Image *, bool) override
	{
		juce::ignoreUnused(titleSpaceX1);

		/* rounded top corners like the original FxSound window:
		 * fill a rounded rect that extends below the bar so only
		 * the top-left/top-right corners show the curve */
		juce::Path p;
		p.addRoundedRectangle(
			0.0f, 0.0f, (float)w,
			(float)h + (float)FX_WINDOW_CORNER_RADIUS,
			(float)FX_WINDOW_CORNER_RADIUS,
			(float)FX_WINDOW_CORNER_RADIUS, true, true, false,
			false);
		g.setColour(fxc(WindowBackground));
		g.fillPath(p);

		/* app mark, then the window name */
		auto &t = FxTheme::get();
		int iconH = 38;
		int iconW = 26;
		int x = 18;
		drawWithinIf(g, t.logo,
			     juce::Rectangle<float>(x, (h - iconH) / 2.0f,
						    iconW, iconH));
		x += iconW + 12;

		int textW = juce::jmax(50, titleSpaceW - x - 12);
		g.setColour(fxc(DefaultText));
		g.setFont(fxFont600(17.0f));
		g.drawText(window.getName(), x, 0, textW, h,
			   juce::Justification::centredLeft, true);
	}

	juce::Button *createDocumentWindowButton(int buttonType) override
	{
		/* GNOME-style window controls: one circular close button,
		 * no minimise/maximise (fixed-size window) */
		if (buttonType == juce::DocumentWindow::closeButton)
			return new FxCloseButton();

		return nullptr;
	}

	/* GNOME-style close button placement: 34 px circle pinned to
	 * the right end of the title bar with a 6 px margin */
	void positionDocumentWindowButtons(
		juce::DocumentWindow &, int titleBarX, int titleBarY,
		int titleBarW, int titleBarH, juce::Button *minimiseButton,
		juce::Button *maximiseButton, juce::Button *closeButton,
		bool positionTitleBarButtonsOnLeft) override
	{
		juce::ignoreUnused(minimiseButton, maximiseButton,
				   positionTitleBarButtonsOnLeft);

		if (closeButton != nullptr) {
			const int size = 34;
			closeButton->setBounds(titleBarX + titleBarW - size - 6,
					       titleBarY +
						       (titleBarH - size) / 2,
					       size, size);
		}
	}

	/* the window itself is a transparent ARGB surface so the
	 * rounded corners can show the desktop through (original app:
	 * setOpaque(false) + fillRoundedRectangle radius 21) */
	void fillResizableWindowBackground(juce::Graphics &g, int w, int h,
					   const juce::BorderSize<int> &,
					   juce::ResizableWindow &) override
	{
		juce::Path p;
		p.addRoundedRectangle(0.0f, 0.0f, (float)w, (float)h,
				      (float)FX_WINDOW_CORNER_RADIUS);
		g.setColour(fxc(WindowBackground));
		g.fillPath(p);
	}

private:
	/* GNOME 50-style close button: flat circle with an X glyph;
	 * the circle only appears on hover/press (Adwaita dark). */
	class FxCloseButton : public FxPressButton
	{
	public:
		FxCloseButton() : FxPressButton("close")
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
		}

		void paintButton(juce::Graphics &g, bool highlighted,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat();
			float scale = pressScale(0.90f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));
			auto circle = r.withSizeKeepingCentre(26.0f, 26.0f);

			if (highlighted || down) {
				g.setColour(fxc(DefaultText,
						down ? 0.16f : 0.10f));
				g.fillEllipse(circle);
			}

			g.setColour(fxc(DefaultText,
					(!isEnabled() || down) ? 0.55f
							       : (highlighted
									  ? 0.95f
									  : 0.80f)));
			auto c = r.getCentre();
			const float d = 5.0f;
			g.drawLine(c.x - d, c.y - d, c.x + d, c.y + d, 1.8f);
			g.drawLine(c.x + d, c.y - d, c.x - d, c.y + d, 1.8f);
		}
	};
};

/* ================================================================== */
/* The FxSound visualizer - native implementation of the upstream      */
/* FxVisualizer.{h,cpp} (same JUCE framework, so                      */
/* the geometry and the history-shift logic are 1:1):                  */
/*   - 10 spectrum bands x 10 history slots; a new value is born in    */
/*     the middle slot and ripples outward symmetrically every tick,   */
/*     100 thin bars, 4 px wide at 9.1 px pitch (virtual 960x120),     */
/*     center-origin, height = raw 0..1 band value * 100               */
/*   - FxTheme dark colors: ControlBackground 0x0f0f0f box, bars       */
/*     GraphHigh 0xd51535 / GraphLow 0xfe566a vertical gradient        */
/*   - alpha 1.0 while audio is live, 0.75 when quiet (upstream        */
/*     calcGradient behavior)                                          */
/* The spectrum feed is the engine's raw band values (the same unscaled */
/* contract the Windows FxController uses; no dB scaling).             */

class FxVisualizer : public juce::Component
{
public:
	static constexpr int WIDTH = 960;
	static constexpr int HEIGHT = 120;

	void push(const float *bands, int n) /* FxVisualizer::update() */
	{
		int n2 = juce::jlimit(0, NUM_BANDS, n);
		for (int i = 0; i < n2; i++) {
			float v = bands[i];
			if (v < 0.0f || v > 1.0f)
				v = 0.0f;
			if (v > 0.02f)
				lastActiveMs =
					juce::Time::getMillisecondCounterHiRes();

			/* history shift - upstream order is significant: the
			 * left half copies first, then the right half (reading
			 * the already-shifted left slots), then the center */
			for (int j = 0; j < NUM_BARS / 2; j++) {
				graph[i * NUM_BARS + j] =
					graph[i * NUM_BARS + j + 1];
				graph[i * NUM_BARS + (NUM_BARS - 1) - j] =
					graph[i * NUM_BARS + j + 1];
			}
			graph[i * NUM_BARS + NUM_BARS / 2] = v;
		}
		repaint();
	}

	void clear() /* FxVisualizer::reset() */
	{
		graph.fill(0.0f);
		repaint();
	}

	void paint(juce::Graphics &g) override
	{
		auto local = getLocalBounds().toFloat();

		double nowMs = juce::Time::getMillisecondCounterHiRes();
		float alpha = (nowMs - lastActiveMs) < 1200.0 ? 1.0f : 0.75f;

		/* box in LOCAL space so the corner radius stays circular */
		g.setColour(fxc(ControlBackground));
		g.fillRoundedRectangle(local, 10.0f);

		g.saveState();
		/* draw the bars in the original virtual 960x120 space */
		g.addTransform(juce::AffineTransform::scale(
			(float)getWidth() / 960.0f,
			(float)getHeight() / 120.0f));

		juce::ColourGradient grad(
			fxc(GraphHigh).withAlpha(alpha), 2.0f, 0.0f,
			fxc(GraphHigh).withAlpha(alpha), 2.0f, 100.0f,
			/* upstream gradient reference frame */
			false);
		grad.addColour(0.5f, fxc(GraphLow).withAlpha(alpha));
		g.setGradientFill(grad);

		juce::Path barsPath;
		float x = 27.0f;
		for (int i = 0; i < NUM_BANDS * NUM_BARS; i++) {
			float v = graph[i] == 0.0f ? 0.01f : graph[i];
			float h = v * 100.0f;
			barsPath.addRectangle(x, 120.0f / 2.0f - h / 2.0f,
					      4.0f, h);
			x += 9.1f;
		}
		g.fillPath(barsPath);
		g.restoreState();
	}

private:
	static constexpr int NUM_BANDS = 10; /* NUM_SPECTRUM_BANDS */
	static constexpr int NUM_BARS = 10;  /* history depth per band */

	std::array<float, NUM_BANDS * NUM_BARS> graph{};
	double lastActiveMs = 0.0;
};

/* ================================================================== */
/* FxAudioControls implementation: the five-effect rack, FxEffects     */

class FxEffects : public juce::Component
{
public:
	static constexpr int LABEL_HEIGHT = 14;
	static constexpr int SLIDER_WIDTH = 160;
	static constexpr int SLIDER_HEIGHT = 18;
	static constexpr int X_MARGIN = 8;
	static constexpr int Y_MARGIN = 21;

	FxEffects()
	{
		static const char *texts[5] = { "Clarity", "Ambience",
			"Surround Sound", "Dynamic Boost", "Bass Boost" };
		/* the Windows GUI names; effect 0 drives DfxDsp Fidelity */

		for (int i = 0; i < 5; i++) {
			labels_[i].setText(texts[i], juce::dontSendNotification);
			labels_[i].setFont(FxTheme::get().normalFont(14.0f));
			labels_[i].setJustificationType(
				juce::Justification::topLeft);
			labels_[i].setColour(juce::Label::textColourId,
					     fxc(DefaultText));
			addAndMakeVisible(labels_[i]);

			effectSliders_[i].setSliderStyle(
				juce::Slider::LinearHorizontal);
			effectSliders_[i].setRange(0, 10, 1.0);
			effectSliders_[i].setTextBoxStyle(
				juce::Slider::NoTextBox, false, 0, 0);
			effectSliders_[i].setMouseCursor(
				juce::MouseCursor::PointingHandCursor);
			effectSliders_[i].setWantsKeyboardFocus(true);
			addAndMakeVisible(effectSliders_[i]);

			valueLabels_[i].setFont(FxTheme::get().normalFont(12.0f));
			valueLabels_[i].setJustificationType(
				juce::Justification::centredLeft);
			valueLabels_[i].setInterceptsMouseClicks(false, false);
			valueLabels_[i].setColour(juce::Label::textColourId,
						  fxc(DefaultText));
			effectSliders_[i].addAndMakeVisible(valueLabels_[i]);
			valueLabels_[i].setBounds(0, (SLIDER_HEIGHT - 12) / 2,
						  FX_SLIDER_THUMB_RADIUS * 3, 12);
		}
	}

	/* FxEffects::update(); engine get is already UI scale 0..10 */
	void update(FXPW *engine)
	{
		for (int i = 0; i < 5; i++)
			setEffectValue(i, fxpw_get_effect_value(engine, i));
	}

	void showValues(bool show) /* values always visible since v2.0 */
	{
		for (int i = 0; i < 5; i++)
			valueLabels_[i].setVisible(show && isEnabled());
	}

	void refreshTheme()
	{
		for (int i = 0; i < 5; i++) {
			labels_[i].setColour(juce::Label::textColourId,
					     fxc(DefaultText));
			valueLabels_[i].setColour(juce::Label::textColourId,
						  fxc(DefaultText));
		}
		repaint();
	}

	/* FxEffectSlider::setEffectValue - slider + readout placement,
	 * NO engine push (only user drags push, via the panel listener) */
	void setEffectValue(int effect, float value)
	{
		auto &s = effectSliders_[effect];
		s.setValue(juce::jlimit(0.0, 10.0, (double)value),
			   juce::dontSendNotification);
		positionValueLabel(effect);
	}

	void positionValueLabel(int effect)
	{
		auto &s = effectSliders_[effect];
		valueLabels_[effect].setText(
			juce::String::formatted("%.0f", s.getValue()),
			juce::dontSendNotification);

		int labelW = valueLabels_[effect].getWidth();
		int thumbX = (int)s.getPositionOfValue(s.getValue());
		int x = thumbX + FX_SLIDER_THUMB_RADIUS + 1;
		if (x + labelW > s.getWidth())
			x = thumbX - FX_SLIDER_THUMB_RADIUS - 1 - labelW;
		x = juce::jlimit(0, juce::jmax(0, s.getWidth() - labelW),
				 x);
		valueLabels_[effect].setBounds(
			valueLabels_[effect].getBounds().withX(x));
	}

	juce::Slider &sliderFor(int effect) { return effectSliders_[effect]; }

	void resized() override
	{
		for (int i = 0; i < 5; i++) {
			labels_[i].setBounds(X_MARGIN + FX_SLIDER_THUMB_RADIUS,
					     Y_MARGIN + i * (14 + 1 + 18 + 10),
					     SLIDER_WIDTH, LABEL_HEIGHT);
			effectSliders_[i].setBounds(
				X_MARGIN,
				Y_MARGIN + i * (14 + 1 + 18 + 10) + 14 + 1,
				SLIDER_WIDTH, SLIDER_HEIGHT);
			valueLabels_[i].setBounds(
				0, (SLIDER_HEIGHT - 12) / 2,
				FX_SLIDER_THUMB_RADIUS * 3, 12);
			positionValueLabel(i);
		}
	}

	void paint(juce::Graphics &g) override
	{
		/* FxAudioControls::paint */
		g.setFillType(juce::FillType(fxc(ControlBackground)));
		g.fillRoundedRectangle(getLocalBounds().toFloat(), 8.0f);
	}

private:
	juce::Label labels_[5];
	juce::Label valueLabels_[5];
	DragOnlySlider effectSliders_[5];
};

/* ================================================================== */
/* FxEqualizer implementation: the band graphic EQ (776x257) with the  */
/* curve and gradient fill (numBands > 10 branch, ours is 31)          */

class FxEqualizer : public juce::Component
{
public:
	static constexpr int WIDTH = 776;
	static constexpr int HEIGHT = 257;
	static constexpr int SLIDER_HEIGHT = 180;
	static constexpr int LABEL_HEIGHT = 12;
	static constexpr int SMALL_FONT = 10;
	static constexpr int ROTARY_SLIDER_HEIGHT = 36;
	static constexpr int X_MARGIN = 16;
	static constexpr int Y_MARGIN = 8;
	static constexpr float MAX_GAIN = 12.0f;

	/* FxEqSlider: vertical gain slider + the readout above the thumb;
	 * right click zeroes the band */
	class FxEqSlider : public DragOnlySlider
	{
	public:
		FxEqSlider()
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
			gainLabel.setFont(FxTheme::get().normalFont(
				(float)LABEL_HEIGHT));
			gainLabel.setJustificationType(
				juce::Justification::centred);
			gainLabel.setInterceptsMouseClicks(false, false);
			gainLabel.setColour(juce::Label::textColourId,
					    fxc(DefaultText));
			addChildComponent(gainLabel);
			setWantsKeyboardFocus(true);
		}

		void setGainValue(float value)
		{
			setValue(value, juce::dontSendNotification);
			updateLabel();
		}

		void updateLabel()
		{
			auto value = (float)getValue();
			gainLabel.setText(juce::String::formatted(
						  value == 0.0f ? "%.0f" : "%+.0f",
						  value),
					  juce::dontSendNotification);

			int y = (int)getPositionOfValue((double)value) -
				(FX_SLIDER_THUMB_RADIUS * 3);
			y = juce::jlimit(0, juce::jmax(0, getHeight() - LABEL_HEIGHT),
					 y);
			gainLabel.setBounds(gainLabel.getBounds().withY(y));
		}

		void showValue(bool show) { gainLabel.setVisible(show); }

		void enablementChanged() override
		{
			setMouseCursor(isEnabled()
					       ? juce::MouseCursor::PointingHandCursor
					       : juce::MouseCursor::NormalCursor);
		}

		void resized() override
		{
			juce::Slider::resized();
			gainLabel.setBounds(0, 0, getWidth(), LABEL_HEIGHT);
			updateLabel();
		}

		void mouseDown(const juce::MouseEvent &event) override
		{
			/* right click resets to flat (upstream) */
			if (event.mods.isRightButtonDown()) {
				engaged_ = false;
				setValue(0.0, juce::sendNotification);
				return;
			}
			DragOnlySlider::mouseDown(event);
		}

		juce::Label gainLabel;
	};

	void initBands(FXPW *engine, int numBands)
	{
		engine_ = engine;
		numBands_ = juce::jlimit(1, 32, numBands);

		for (int i = 0; i < numBands_; i++) {
			auto &s = bandBoosts_[i];
			s.setSliderStyle(juce::Slider::LinearVertical);
			s.setRange(-MAX_GAIN, MAX_GAIN, 1.0);
			s.setTextBoxStyle(juce::Slider::NoTextBox, false, 0, 0);
			s.setValue(fxpw_get_eq_band_boostcut(engine_, i),
				   juce::dontSendNotification);
			addAndMakeVisible(s);

			/* upstream label format for numBands >= 15:
			 * two lines, value over unit */
			freqLabels_[i].setMinimumHorizontalScale(0.4f);
			freqLabels_[i].setFont(
				FxTheme::get().normalFont((float)SMALL_FONT));
			freqLabels_[i].setJustificationType(
				juce::Justification::centredTop);
			freqLabels_[i].setColour(juce::Label::textColourId,
						 fxc(DefaultText));
			updateFrequencyLabel(i);
			addAndMakeVisible(freqLabels_[i]);

			s.updateLabel();
			s.showValue(true);
		}
	}

	void refreshValues()
	{
		for (int i = 0; i < numBands_; i++) {
			/* A restored session can carry the same band
			 * frequencies; refresh the labels as well so the
			 * panel always mirrors the engine. */
			updateFrequencyLabel(i);
			auto v = fxpw_get_eq_band_boostcut(engine_, i);
			if (v >= -MAX_GAIN && v <= MAX_GAIN)
				bandBoosts_[i].setGainValue(v);
		}
	}

	void updateFrequencyLabel(int i)
	{
		auto freq = fxpw_get_eq_band_freq(engine_, i);
		juce::String text;
		if (freq >= 10000.0f)
			text = juce::String::formatted("%.0f\nkHz",
						       freq / 1000.0f);
		else if (freq >= 1000.0f)
			text = juce::String::formatted("%.1f\nkHz",
						       freq / 1000.0f);
		else
			text = juce::String::formatted("%.0f\nHz", freq);
		freqLabels_[i].setText(text, juce::dontSendNotification);
	}

	FxEqSlider &sliderForBand(int b) { return bandBoosts_[b]; }
	int numBands() const { return numBands_; }

	void refreshTheme()
	{
		for (int i = 0; i < numBands_; i++) {
			bandBoosts_[i].gainLabel.setColour(
				juce::Label::textColourId, fxc(DefaultText));
			freqLabels_[i].setColour(juce::Label::textColourId,
						 fxc(DefaultText));
		}
		repaint();
	}

	void resized() override
	{
		/* FxEqualizer::resized, the >10-bands branch */
		int per = (getLocalBounds().getWidth() - X_MARGIN * 2) / numBands_;

		int x = X_MARGIN;
		for (int i = 0; i < numBands_; i++) {
			freqLabels_[i].setFont(
				FxTheme::get().normalFont((float)SMALL_FONT));
			bandBoosts_[i].setBounds(
				x + (per - FX_SLIDER_THUMB_RADIUS * 4) / 2,
				Y_MARGIN, FX_SLIDER_THUMB_RADIUS * 4,
				SLIDER_HEIGHT + ROTARY_SLIDER_HEIGHT);
			freqLabels_[i].setBounds(
				x, bandBoosts_[i].getBottom() + 6, per,
				LABEL_HEIGHT * 2);
			x += per;
		}
	}

	void paint(juce::Graphics &g) override
	{
		g.setFillType(juce::FillType(fxc(ControlBackground)));
		g.fillRoundedRectangle(getLocalBounds().toFloat(), 8.0f);

		juce::Path path;

		juce::Colour lineColour = fxc(SliderTrack);
		juce::Colour gradientColour1 = fxc(EqStart, 0.34f);
		juce::Colour gradientColour2 = fxc(EqEnd, 0.0f);

		if (!isEnabled()) {
			lineColour = lineColour.withSaturation(0.0f);
			gradientColour1 = gradientColour1.withSaturation(0.0f);
			gradientColour2 = gradientColour2.withSaturation(0.0f);
		}

		/* connecting polyline, segment by segment (upstream) */
		for (int i = 0; i + 1 < numBands_; i++) {
			auto y0 = bandBoosts_[i].getPositionOfValue(
					  bandBoosts_[i].getValue()) +
				  Y_MARGIN;
			auto y1 = bandBoosts_[i + 1].getPositionOfValue(
					  bandBoosts_[i + 1].getValue()) +
				  Y_MARGIN;

			auto x0 = bandBoosts_[i].getX() +
				  bandBoosts_[i].getWidth() / 2;
			auto x1 = bandBoosts_[i + 1].getX() +
				  bandBoosts_[i + 1].getWidth() / 2;

			path.addLineSegment(
				juce::Line<float>((float)x0, (float)y0,
						  (float)x1, (float)y1),
				1.0);

			g.setColour(lineColour);
			g.strokePath(path, juce::PathStrokeType(1.0f));
			path.clear();
		}

		/* gradient fill under the curve (upstream shape) */
		for (int i = 0; i < numBands_; i++) {
			auto x = bandBoosts_[i].getX() +
				 bandBoosts_[i].getWidth() / 2;
			auto y = bandBoosts_[i].getPositionOfValue(
					 bandBoosts_[i].getValue()) +
				 Y_MARGIN;

			if (i == 0)
				path.startNewSubPath(juce::Point<float>(
					(float)x, bandBoosts_[i].getBottom() -
							  (float)FX_SLIDER_THUMB_RADIUS));

			path.lineTo((float)x, (float)y);

			if (i == numBands_ - 1)
				path.lineTo(juce::Point<float>(
					(float)x, bandBoosts_[i].getBottom() -
							  (float)FX_SLIDER_THUMB_RADIUS));
		}
		path.closeSubPath();

		juce::ColourGradient gradient(
			gradientColour1, 0.0f, bandBoosts_[0].getY(),
			gradientColour2, 0.0f, bandBoosts_[0].getBottom(),
			false);
		g.setFillType(juce::FillType(gradient));
		g.fillPath(path);
	}

private:
	juce::Label freqLabels_[32];
	FxEqSlider bandBoosts_[32];
	FXPW *engine_ = nullptr;
	int numBands_ = 0;
};

/* ================================================================== */
/* Settings sheet: theme (dark/light + custom accent) and preset
 * import/export, shown as a modal overlay over the pro view.          */

class FxAccentPicker : public juce::Component
{
public:
	FxAccentPicker(juce::Colour initial,
		       std::function<void(juce::Colour)> changed)
		: colour_(initial), onChange(std::move(changed))
	{
		auto setupLabel = [this](juce::Label &l, const char *text) {
			l.setText(text, juce::dontSendNotification);
			l.setFont(FxTheme::get().smallFont(12.0f));
			l.setColour(juce::Label::textColourId, fxc(DefaultText));
			l.setJustificationType(juce::Justification::centredLeft);
			addAndMakeVisible(l);
		};
		setupLabel(hueLabel_, "Hue");
		setupLabel(satLabel_, "Saturation");
		setupLabel(briLabel_, "Brightness");

		for (auto *s : { &hue_, &sat_, &bri_ }) {
			s->setSliderStyle(juce::Slider::LinearHorizontal);
			s->setTextBoxStyle(juce::Slider::TextBoxRight, false,
					   44, 20);
			s->setMouseCursor(juce::MouseCursor::PointingHandCursor);
			addAndMakeVisible(s);
		}
		hue_.setRange(0.0, 360.0, 1.0);
		sat_.setRange(0.0, 100.0, 1.0);
		bri_.setRange(0.0, 100.0, 1.0);
		hue_.setValue(initial.getHue() * 360.0f,
			      juce::dontSendNotification);
		sat_.setValue(initial.getSaturation() * 100.0f,
			      juce::dontSendNotification);
		bri_.setValue(initial.getBrightness() * 100.0f,
			      juce::dontSendNotification);

		hue_.onValueChange = [this] { update(); };
		sat_.onValueChange = [this] { update(); };
		bri_.onValueChange = [this] { update(); };

		setSize(320, 210);
	}

	void paint(juce::Graphics &g) override
	{
		g.setColour(fxc(DefaultText));
		g.setFont(FxTheme::get().normalFont(14.0f));
		g.drawText("Custom accent", 82, 18, 200, 24,
			   juce::Justification::centredLeft, false);

		auto swatch = juce::Rectangle<float>(16.0f, 16.0f, 48.0f, 48.0f);
		g.setColour(colour_);
		g.fillRoundedRectangle(swatch, 8.0f);
		g.setColour(fxc(Outline));
		g.drawRoundedRectangle(swatch.reduced(0.5f), 8.0f, 1.0f);
	}

	void resized() override
	{
		hueLabel_.setBounds(16, 74, 80, 26);
		satLabel_.setBounds(16, 108, 80, 26);
		briLabel_.setBounds(16, 142, 80, 26);
		hue_.setBounds(98, 74, 206, 26);
		sat_.setBounds(98, 108, 206, 26);
		bri_.setBounds(98, 142, 206, 26);
	}

private:
	void update()
	{
		colour_ = juce::Colour::fromHSV(
			(float)hue_.getValue() / 360.0f,
			(float)sat_.getValue() / 100.0f,
			(float)bri_.getValue() / 100.0f, 1.0f);
		repaint();
		if (onChange)
			onChange(colour_);
	}

	juce::Label hueLabel_, satLabel_, briLabel_;
	juce::Slider hue_, sat_, bri_;
	juce::Colour colour_;
	std::function<void(juce::Colour)> onChange;
};

class FxSettingsOverlay : public juce::Component,
			  private juce::Timer
{
public:
	std::function<void()> onClose;
	std::function<void(int)> onThemeMode;
	std::function<void(juce::Colour)> onAccent;
	std::function<void()> onResetAccent;
	std::function<void()> onImport;
	std::function<void()> onExport;
	std::function<void(const juce::String &)> onSave;
	std::function<juce::String()> onSaveSuggestion;
	std::function<bool(const juce::String &)> onSaveNameExists;
	std::function<void()> onQuit;
	std::function<void(bool)> onAutostart;

	FxSettingsOverlay()
	{
		setWantsKeyboardFocus(true);
		setOpaque(false);

		title_.setText("Settings", juce::dontSendNotification);
		title_.setFont(FxTheme::get().titleFont(21.0f));
		title_.setJustificationType(
			juce::Justification::centredLeft);
		addAndMakeVisible(title_);

		hint_.setText("Save the current effects and EQ as a custom "
			      "preset, or import/export .fac files.",
			      juce::dontSendNotification);
		hint_.setFont(FxTheme::get().smallFont(12.5f));
		hint_.setJustificationType(
			juce::Justification::topLeft);
		hint_.setMinimumHorizontalScale(0.8f);
		addAndMakeVisible(hint_);

		status_.setFont(FxTheme::get().normalFont(13.0f));
		status_.setJustificationType(
			juce::Justification::centredLeft);
		addAndMakeVisible(status_);

		accentLabel_.setText("Accent", juce::dontSendNotification);
		accentLabel_.setFont(FxTheme::get().normalFont(13.5f));
		accentLabel_.setJustificationType(
			juce::Justification::centredLeft);
		addAndMakeVisible(accentLabel_);

		close_.onClick = [this] {
			if (onClose)
				onClose();
		};
		addAndMakeVisible(close_);

		dark_.setButtonText("Dark");
		dark_.onClick = [this] {
			if (onThemeMode)
				onThemeMode(ThemeDark);
		};
		addAndMakeVisible(dark_);

		light_.setButtonText("Light");
		light_.onClick = [this] {
			if (onThemeMode)
				onThemeMode(ThemeLight);
		};
		addAndMakeVisible(light_);

		static const uint32_t choices[6] = {
			0xffe63462, /* default pink */
			0xff23b6eb, /* default blue */
			0xff34c759, 0xff9b59ff, 0xffff9f0a, 0xff00c7be
		};
		for (int i = 0; i < 6; i++) {
			swatches_[i].setSwatch(juce::Colour(choices[i]));
			swatches_[i].onClick = [this, i] {
				if (onAccent)
					onAccent(juce::Colour(choices[i]));
			};
			addAndMakeVisible(swatches_[i]);
		}

		accentButton_.onClick = [this] { openAccentPicker(); };
		addAndMakeVisible(accentButton_);

		reset_.setButtonText("Default");
		reset_.onClick = [this] {
			if (onResetAccent)
				onResetAccent();
		};
		addAndMakeVisible(reset_);

		import_.setButtonText("Import presets...");
		import_.onClick = [this] {
			if (onImport)
				onImport();
		};
		addAndMakeVisible(import_);

		save_.setButtonText("Save preset...");
		save_.onClick = [this] { openSavePrompt(); };
		addAndMakeVisible(save_);

		export_.setButtonText("Export preset...");
		export_.onClick = [this] {
			if (onExport)
				onExport();
		};
		addAndMakeVisible(export_);

		/* Closing the window keeps the DSP running in the background;
		 * this is the explicit way to stop the engine and exit. */
		quit_.setButtonText("Quit SauceFX");
		quit_.onClick = [this] {
			if (onQuit)
				onQuit();
		};
		addAndMakeVisible(quit_);

		startupLabel_.setText("Start at login",
				      juce::dontSendNotification);
		startupLabel_.setFont(FxTheme::get().normalFont(13.5f));
		startupLabel_.setJustificationType(
			juce::Justification::centredLeft);
		addAndMakeVisible(startupLabel_);

		startupHint_.setText(
			"Runs hidden in the background when you log in.",
			juce::dontSendNotification);
		startupHint_.setFont(FxTheme::get().smallFont(11.5f));
		startupHint_.setJustificationType(
			juce::Justification::topLeft);
		startupHint_.setMinimumHorizontalScale(0.8f);
		addAndMakeVisible(startupHint_);

		autostart_.onClick = [this] {
			if (onAutostart)
				onAutostart(autostart_.getToggleState());
		};
		addAndMakeVisible(autostart_);

		refreshFromTheme();
	}

	/* The sheet is an occasional surface: a short opacity transition
	 * is enough to make it feel attached to the gear button without
	 * delaying the controls. Opening is 180 ms; closing is 120 ms
	 * because the user already knows what they dismissed. */
	void showAnimated()
	{
		if (isVisible() && !closing_)
			return;
		if (!isVisible()) {
			setAlpha(0.0f);
			setVisible(true);
		}
		closing_ = false;
		animationStartMs_ = juce::Time::getMillisecondCounterHiRes();
		animationStartAlpha_ = getAlpha();
		setInterceptsMouseClicks(true, true);
		startTimerHz(60);
	}

	void hideAnimated()
	{
		if (!isVisible() || closing_)
			return;
		closing_ = true;
		animationStartMs_ = juce::Time::getMillisecondCounterHiRes();
		animationStartAlpha_ = getAlpha();
		/* Let the fade finish without swallowing clicks meant for
		 * the panel underneath. */
		setInterceptsMouseClicks(false, false);
		startTimerHz(60);
	}

	bool isOpen() const { return isVisible() && !closing_; }

	void setAutostart(bool enabled)
	{
		autostart_.setToggleState(enabled,
					  juce::dontSendNotification);
	}

	void refreshFromTheme()
	{
		dark_.setToggleState(fx_theme_mode_ == ThemeDark,
				     juce::dontSendNotification);
		light_.setToggleState(fx_theme_mode_ == ThemeLight,
				      juce::dontSendNotification);

		auto active = juce::Colour(0xff000000u | fx_accent_argb_);
		for (int i = 0; i < 6; i++) {
			bool selected = fx_accent_custom_ &&
					swatches_[i].getSwatch() == active;
			swatches_[i].setToggleState(
				selected, juce::dontSendNotification);
		}
		accentButton_.setAccent(fx_accent_custom_ ? active
							  : fxc(ImageButton));
		reset_.setToggleState(!fx_accent_custom_,
				      juce::dontSendNotification);

		title_.setColour(juce::Label::textColourId, fxc(DefaultText));
		hint_.setColour(juce::Label::textColourId, fxc(HintText));
		startupLabel_.setColour(juce::Label::textColourId,
					fxc(DefaultText));
		startupHint_.setColour(juce::Label::textColourId,
				       fxc(HintText));
		accentLabel_.setColour(juce::Label::textColourId,
				       fxc(DefaultText));
		status_.setColour(
			juce::Label::textColourId,
			fxc(statusIsError_ ? InvalidTextBorder : DefaultText));
		repaint();
	}

	void setStatus(const juce::String &text, bool error = false)
	{
		statusIsError_ = error;
		status_.setText(text, juce::dontSendNotification);
		status_.setColour(juce::Label::textColourId,
				  fxc(error ? InvalidTextBorder : DefaultText));
	}

	void paint(juce::Graphics &g) override
	{
		g.fillAll(juce::Colours::black.withAlpha(
			fx_theme_mode_ == ThemeLight ? 0.30f : 0.55f));

		g.setColour(fxc(WindowBackground));
		g.fillRoundedRectangle(card_.toFloat(), 14.0f);
		g.setColour(fxc(Outline));
		g.drawRoundedRectangle(card_.toFloat().reduced(0.5f), 14.0f,
				       1.0f);

		auto x = (float)card_.getX();
		auto y = (float)card_.getY();
		auto w = (float)card_.getWidth();

		g.setColour(fxc(Outline));
		g.drawLine(x + 28.0f, y + 66.0f, x + w - 28.0f, y + 66.0f,
			   1.0f);
		g.drawLine(x + 28.0f, y + 300.0f, x + w - 28.0f, y + 300.0f,
			   1.0f);

		g.setColour(fxc(HintText));
		g.setFont(FxTheme::get().normalFont(12.0f));
		g.drawText("APPEARANCE", card_.getX() + 28, card_.getY() + 78,
			   300, 16, juce::Justification::centredLeft, true);
		g.drawText("PRESETS", card_.getX() + 28, card_.getY() + 200,
			   300, 16, juce::Justification::centredLeft, true);
	}

	void resized() override
	{
		const int w = 640;
		const int h = 410;
		card_ = juce::Rectangle<int>((getWidth() - w) / 2,
					     (getHeight() - h) / 2, w, h);

		const int x = card_.getX();
		const int y = card_.getY();

		title_.setBounds(x + 28, y + 18, 300, 30);
		close_.setBounds(x + w - 28 - 28, y + 18, 28, 28);

		dark_.setBounds(x + 28, y + 102, 120, 34);
		light_.setBounds(x + 158, y + 102, 120, 34);

		accentLabel_.setBounds(x + 28, y + 148, 64, 30);

		for (int i = 0; i < 6; i++)
			swatches_[i].setBounds(x + 96 + i * 34, y + 148,
					       28, 30);
		accentButton_.setBounds(x + 306, y + 148, 104, 30);
		reset_.setBounds(x + 418, y + 148, 82, 30);

		import_.setBounds(x + 28, y + 222, 146, 34);
		save_.setBounds(x + 182, y + 222, 146, 34);
		export_.setBounds(x + 336, y + 222, 146, 34);
		quit_.setBounds(x + w - 28 - 112, y + 222, 112, 34);

		hint_.setBounds(x + 28, y + 266, w - 56, 34);

		startupLabel_.setBounds(x + 28, y + 310, 320, 24);
		startupHint_.setBounds(x + 28, y + 334, w - 56, 16);
		autostart_.setBounds(x + w - 28 - 46, y + 310, 46, 26);

		status_.setBounds(x + 28, y + 356, w - 56, 22);
	}

	bool keyPressed(const juce::KeyPress &key) override
	{
		if (closing_)
			return true;
		if (key == juce::KeyPress::escapeKey) {
			if (onClose)
				onClose();
			return true;
		}
		return juce::Component::keyPressed(key);
	}

	void mouseDown(const juce::MouseEvent &e) override
	{
		if (!closing_ && !card_.contains(e.getPosition()) && onClose)
			onClose();
	}

private:
	class ToggleSwitch : public FxPressButton
	{
	public:
		ToggleSwitch() : FxPressButton("autostart")
		{
			setClickingTogglesState(true);
			setWantsKeyboardFocus(true);
			setMouseCursor(
				juce::MouseCursor::PointingHandCursor);
		}

		void paintButton(juce::Graphics &g, bool over,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat().reduced(0.5f);
			float scale = pressScale(0.97f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));

			const bool on = getToggleState();
			auto bg = on ? fxc(ImageButton)
				     : fxc(ControlBackground);
			if (down)
				bg = on ? bg.darker(0.12f) : bg.brighter(0.18f);
			else if (over)
				bg = on ? bg.brighter(0.08f)
					: bg.brighter(0.10f);

			g.setColour(bg);
			g.fillRoundedRectangle(r, r.getHeight() / 2.0f);
			g.setColour(on ? fxc(ImageButton) : fxc(Outline));
			g.drawRoundedRectangle(r, r.getHeight() / 2.0f,
					       1.0f);

			const float inset = 4.0f;
			const float knob = r.getHeight() - inset * 2.0f;
			const float cx = on
				? r.getRight() - inset - knob / 2.0f
				: r.getX() + inset + knob / 2.0f;
			g.setColour(on ? fxc(HighlightedText)
				       : fxc(HintText));
			g.fillEllipse(juce::Rectangle<float>(knob, knob)
					      .withCentre({ cx,
							    r.getCentreY() }));
		}
	};

	class FlatButton : public FxPressButton
	{
	public:
		explicit FlatButton(const juce::String &text = {})
			: FxPressButton(text)
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
		}

		void paintButton(juce::Graphics &g, bool over,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat().reduced(0.5f);
			float scale = pressScale(0.985f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));
			auto bg = fxc(ControlBackground);
			if (down)
				bg = bg.brighter(0.18f);
			else if (over)
				bg = bg.brighter(0.10f);

			g.setColour(bg);
			g.fillRoundedRectangle(r, 6.0f);
			g.setColour(getToggleState() ? fxc(ImageButton)
						     : fxc(Outline));
			g.drawRoundedRectangle(
				r, 6.0f, getToggleState() ? 1.5f : 1.0f);

			g.setColour(isEnabled() ? fxc(DefaultText)
						: fxc(HintText));
			g.setFont(FxTheme::get().normalFont(13.5f));
			g.drawText(getButtonText(), getLocalBounds(),
				   juce::Justification::centred, true);
		}
	};

	/* A small themed call-out for naming a new custom preset. The
	 * existing preset list is never overwritten from here; the panel
	 * owns the target path and duplicate handling. */
	class SavePresetPrompt : public juce::Component
	{
	public:
		SavePresetPrompt(
			const juce::String &initialName,
			std::function<void(const juce::String &)> save,
			std::function<bool(const juce::String &)> nameExists)
			: onSave_(std::move(save)),
			  nameExists_(std::move(nameExists))
		{
			setOpaque(false);

			nameEditor_.setFont(FxTheme::get().normalFont(14.0f));
			nameEditor_.setColour(
				juce::TextEditor::backgroundColourId,
				fxc(DefaultFill));
			nameEditor_.setColour(
				juce::TextEditor::outlineColourId,
				fxc(Outline));
			nameEditor_.setColour(
				juce::TextEditor::focusedOutlineColourId,
				fxc(ImageButton));
			nameEditor_.setColour(
				juce::TextEditor::textColourId,
				fxc(DefaultText));
			nameEditor_.setColour(
				juce::TextEditor::highlightColourId,
				fxc(ImageButton).withAlpha(0.35f));
			nameEditor_.setColour(
				juce::TextEditor::highlightedTextColourId,
				fxc(HighlightedText));
			nameEditor_.setIndents(10, 7);
			nameEditor_.setText(initialName, false);
			nameEditor_.setSelectAllWhenFocused(true);
			nameEditor_.setEscapeAndReturnKeysConsumed(false);
			nameEditor_.onReturnKey = [this] { submit(); };
			nameEditor_.onEscapeKey = [this] { dismiss(); };
			nameEditor_.onTextChange = [this] {
				if (!replaceMode_ && error_.isEmpty())
					return;
				replaceMode_ = false;
				error_.clear();
				save_.setButtonText("Save");
				repaint();
			};
			addAndMakeVisible(nameEditor_);

			save_.setButtonText("Save");
			save_.onClick = [this] { submit(); };
			addAndMakeVisible(save_);

			cancel_.setButtonText("Cancel");
			cancel_.onClick = [this] { dismiss(); };
			addAndMakeVisible(cancel_);

			setSize(380, 178);
		}

		void paint(juce::Graphics &g) override
		{
			g.setColour(fxc(DefaultText));
			g.setFont(FxTheme::get().titleFont(18.0f));
			g.drawText("Save preset", 0, 2, getWidth(), 26,
				   juce::Justification::centredLeft, false);

			g.setColour(fxc(HintText));
			g.setFont(FxTheme::get().smallFont(12.5f));
			g.drawText("Store the current effects and EQ in your "
				   "preset library.",
				   0, 28, getWidth(), 20,
				   juce::Justification::centredLeft, true);

			g.setColour(error_.isNotEmpty()
					    ? fxc(replaceMode_ ? ImageButton
							      : InvalidTextBorder)
					    : fxc(HintText));
			g.setFont(FxTheme::get().smallFont(12.0f));
			g.drawText(error_, 0, 112, getWidth(), 18,
				   juce::Justification::centredLeft, true);
		}

		void resized() override
		{
			nameEditor_.setBounds(0, 58, getWidth(), 36);
			cancel_.setBounds(getWidth() - 200, 134, 92, 34);
			save_.setBounds(getWidth() - 100, 134, 100, 34);
		}

		void parentHierarchyChanged() override { requestFocus(); }

		void visibilityChanged() override { requestFocus(); }

	private:
		void requestFocus()
		{
			if (!isShowing() || focusRequested_)
				return;
			focusRequested_ = true;
			juce::MessageManager::callAsync(
				[safe = juce::Component::SafePointer<
					 SavePresetPrompt>(this)] {
					if (safe != nullptr) {
						safe->nameEditor_
							.grabKeyboardFocus();
						safe->nameEditor_.selectAll();
					}
				});
		}

		void submit()
		{
			auto name = nameEditor_.getText().trim();
			if (name.isEmpty() || name == "." || name == "..") {
				replaceMode_ = false;
				error_ = "Enter a name for this preset.";
				save_.setButtonText("Save");
				repaint();
				nameEditor_.grabKeyboardFocus();
				return;
			}

			if (!replaceMode_ && nameExists_ &&
			    nameExists_(name)) {
				replaceMode_ = true;
				error_ = "This custom preset already exists. "
					 "Click Replace to overwrite it.";
				save_.setButtonText("Replace");
				repaint();
				return;
			}

			auto save = onSave_;
			dismiss();
			if (save)
				save(name);
		}

		void dismiss()
		{
			if (auto *box =
				    findParentComponentOfClass<juce::CallOutBox>())
				box->dismiss();
		}

		juce::TextEditor nameEditor_;
		FlatButton save_, cancel_;
		juce::String error_;
		bool replaceMode_ = false;
		bool focusRequested_ = false;
		std::function<void(const juce::String &)> onSave_;
		std::function<bool(const juce::String &)> nameExists_;
	};

	class ChoiceButton : public FxPressButton
	{
	public:
		explicit ChoiceButton(const juce::String &text = {})
			: FxPressButton(text)
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
		}

		void paintButton(juce::Graphics &g, bool over,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat().reduced(0.5f);
			float scale = pressScale(0.985f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));
			const bool on = getToggleState();
			auto bg = on ? fxc(ImageButton) : fxc(ControlBackground);
			if (down)
				bg = on ? bg.darker(0.12f) : bg.brighter(0.18f);
			else if (over)
				bg = on ? bg.brighter(0.08f) : bg.brighter(0.10f);

			g.setColour(bg);
			g.fillRoundedRectangle(r, 6.0f);
			if (!on) {
				g.setColour(fxc(Outline));
				g.drawRoundedRectangle(r, 6.0f, 1.0f);
			}
			g.setColour(on ? fxc(HighlightedText)
				       : fxc(DefaultText));
			g.setFont(FxTheme::get().normalFont(14.0f));
			g.drawText(getButtonText(), getLocalBounds(),
				   juce::Justification::centred, true);
		}
	};

	class SwatchButton : public FxPressButton
	{
	public:
		SwatchButton() : FxPressButton("accent")
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
		}

		void setSwatch(juce::Colour c)
		{
			swatch_ = c;
			repaint();
		}
		juce::Colour getSwatch() const { return swatch_; }

		void paintButton(juce::Graphics &g, bool over,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat();
			float scale = pressScale(0.90f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));
			if (over || down) {
				g.setColour(fxc(DefaultText,
						down ? 0.16f : 0.08f));
				g.fillEllipse(r.withSizeKeepingCentre(28.0f,
								      28.0f));
			}
			auto c = r.withSizeKeepingCentre(18.0f, 18.0f);
			g.setColour(swatch_);
			g.fillEllipse(c);
			if (getToggleState()) {
				g.setColour(fxc(DefaultText));
				g.drawEllipse(c.expanded(3.0f), 1.5f);
			}
		}

	private:
		juce::Colour swatch_;
	};

	class AccentButton : public FxPressButton
	{
	public:
		AccentButton() : FxPressButton("Custom...")
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
		}

		void setAccent(juce::Colour c)
		{
			accent_ = c;
			repaint();
		}

		void paintButton(juce::Graphics &g, bool over,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat().reduced(0.5f);
			float scale = pressScale(0.985f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));
			auto bg = fxc(ControlBackground);
			if (down)
				bg = bg.brighter(0.18f);
			else if (over)
				bg = bg.brighter(0.10f);
			g.setColour(bg);
			g.fillRoundedRectangle(r, 6.0f);
			g.setColour(fxc(Outline));
			g.drawRoundedRectangle(r, 6.0f, 1.0f);

			g.setColour(accent_);
			g.fillEllipse(8.0f, (float)getHeight() / 2.0f - 7.0f,
				      14.0f, 14.0f);
			g.setColour(fxc(DefaultText));
			g.setFont(FxTheme::get().normalFont(13.5f));
			g.drawText("Custom...", 28, 0, getWidth() - 32,
				   getHeight(),
				   juce::Justification::centredLeft, true);
		}

	private:
		juce::Colour accent_;
	};

	class OverlayCloseButton : public FxPressButton
	{
	public:
		OverlayCloseButton() : FxPressButton("close")
		{
			setMouseCursor(juce::MouseCursor::PointingHandCursor);
		}

		void paintButton(juce::Graphics &g, bool over,
				 bool down) override
		{
			auto r = getLocalBounds().toFloat();
			float scale = pressScale(0.90f);
			g.addTransform(fxScaleAround(scale, r.getCentre()));
			auto circle = r.withSizeKeepingCentre(26.0f, 26.0f);
			if (over || down) {
				g.setColour(fxc(DefaultText,
						down ? 0.16f : 0.10f));
				g.fillEllipse(circle);
			}
			g.setColour(fxc(DefaultText,
					down ? 0.60f : (over ? 0.95f : 0.80f)));
			auto c = r.getCentre();
			const float d = 5.0f;
			g.drawLine(c.x - d, c.y - d, c.x + d, c.y + d, 1.8f);
			g.drawLine(c.x + d, c.y - d, c.x - d, c.y + d, 1.8f);
		}
	};

	void openSavePrompt()
	{
		if (onSave == nullptr)
			return;

		auto safe = juce::Component::SafePointer<FxSettingsOverlay>(
			this);
		auto suggestion = onSaveSuggestion
			? onSaveSuggestion()
			: juce::String("My preset");
		auto prompt = std::make_unique<SavePresetPrompt>(
			suggestion,
			[safe](const juce::String &name) {
				if (safe != nullptr && safe->onSave)
					safe->onSave(name);
			},
			[safe](const juce::String &name) {
				return safe != nullptr && safe->onSaveNameExists
					&& safe->onSaveNameExists(name);
			});

		auto &box = juce::CallOutBox::launchAsynchronously(
			std::move(prompt), save_.getScreenBounds(), nullptr);
		box.setDismissalMouseClicksAreAlwaysConsumed(true);
	}

	void openAccentPicker()
	{
		auto safe = juce::Component::SafePointer<FxSettingsOverlay>(
			this);
		auto picker = std::make_unique<FxAccentPicker>(
			fx_accent_custom_
				? juce::Colour(fx_accent_argb_)
				: fxc(ImageButton),
			[safe](juce::Colour c) {
				if (safe != nullptr && safe->onAccent)
					safe->onAccent(c);
			});
		juce::CallOutBox::launchAsynchronously(
			std::move(picker), accentButton_.getScreenBounds(),
			nullptr);
	}

	juce::Rectangle<int> card_;
	juce::Label title_, hint_, status_, accentLabel_;
	juce::Label startupLabel_, startupHint_;
	bool statusIsError_ = false;

	OverlayCloseButton close_;
	ChoiceButton dark_, light_;
	SwatchButton swatches_[6];
	AccentButton accentButton_;
	FlatButton reset_;
	FlatButton import_, save_, export_;
	FlatButton quit_;
	ToggleSwitch autostart_;

	void timerCallback() override
	{
		constexpr double fadeInMs = 180.0;
		constexpr double fadeOutMs = 120.0;
		const double duration = closing_ ? fadeOutMs : fadeInMs;
		const double elapsed = juce::Time::getMillisecondCounterHiRes() -
				       animationStartMs_;
		const float t = (float)juce::jlimit(
			0.0, 1.0, elapsed / duration);
		const float eased = fxEaseOutCubic(t);

		const float alpha = closing_
			? animationStartAlpha_ * (1.0f - eased)
			: animationStartAlpha_ +
				  (1.0f - animationStartAlpha_) * eased;
		setAlpha(juce::jlimit(0.0f, 1.0f, alpha));

		if (t >= 1.0f) {
			stopTimer();
			if (closing_) {
				closing_ = false;
				setVisible(false);
				setAlpha(1.0f);
			} else {
				setAlpha(1.0f);
			}
		}
	}

	bool closing_ = false;
	double animationStartMs_ = 0.0;
	float animationStartAlpha_ = 0.0f;
};

/* ================================================================== */
/* The FxPowerButton from the original title bar                       */

class FxPowerButton : public FxPressButton
{
public:
	FxPowerButton() : FxPressButton("powerButton")
	{
		setMouseCursor(juce::MouseCursor::PointingHandCursor);
		setWantsKeyboardFocus(true);
	}

	void paintButton(juce::Graphics &g, bool, bool) override
	{
		auto &t = FxTheme::get();
		auto &d = powerState ? t.powerOn : t.powerOff;
		auto imageArea = juce::Rectangle<int>(0, 0, imageWidth_,
						      imageWidth_);
		auto bounds = getLocalBounds();
		juce::RectanglePlacement placement(
			juce::RectanglePlacement::xMid |
			juce::RectanglePlacement::yMid |
			juce::RectanglePlacement::doNotResize);
		auto dest = placement.appliedTo(imageArea, bounds).toFloat();

		float scale = pressScale(0.92f);
		g.addTransform(fxScaleAround(scale, dest.getCentre()));

		if (d)
			d->drawWithin(g, dest,
				      juce::RectanglePlacement::centred,
				      isMouseButtonDown() ? 0.7f : 1.0f);
	}

	void setPowerState(bool on) { powerState = on; repaint(); }
	bool getPowerState() const { return powerState; }

private:
	bool powerState = false;
	int imageWidth_ = 24;
};

/* ================================================================== */
/* The FxProView layout, wired to the fxpw bridge                      */

class SauceFXPanel : public juce::Component,
		     private juce::ComboBox::Listener,
		     private juce::Slider::Listener,
		     private juce::Timer
{
public:
	/* set by the window: apply a live theme change to the whole app */
	std::function<void(int, bool, uint32_t)> onThemeChange;
	std::function<void(bool)> onSettingsVisibilityChanged;

	/* set by the web host: library changes and user-facing result messages */
	std::function<void()> onWebStateChanged;
	std::function<void(const juce::String &, bool)> onWebNotification;

	SauceFXPanel(FXPW *engineIn)
		: engine(engineIn)
	{
		/* preset list (40,32,470,40) - factory + imported presets */
		presetBox.setJustificationType(
			juce::Justification::centredLeft);
		presetBox.setWantsKeyboardFocus(true);
		presetBox.addListener(this);
		rebuildPresetList();

		/* Restore the last selected preset plus the live effect/EQ
		 * state. The state lives beside settings.txt, never inside
		 * the user's .fac library. A missing/corrupt session falls
		 * back to the factory Default preset. */
		auto savedSession = loadPresetSession();
		int selectedId = 0;
		juce::File selectedFile;

		if (savedSession.valid &&
		    savedSession.presetPath.isNotEmpty()) {
			juce::File wanted(savedSession.presetPath);
			for (int i = 0; i < presetFiles.size(); i++) {
				if (juce::File(presetFiles[i]) == wanted) {
					selectedId = i + 1;
					selectedFile = wanted;
					break;
				}
			}
		}

		if (selectedId == 0) {
			/* first run, deleted preset, or invalid session */
			selectedId = 1;
			for (int i = 1; i <= presetBox.getNumItems(); i++) {
				if (presetBox.getItemText(i - 1) == "Default") {
					selectedId = i;
					break;
				}
			}
			if (selectedId > 0 &&
			    selectedId <= presetFiles.size())
				selectedFile =
					juce::File(presetFiles[selectedId - 1]);
		}

		if (selectedId > 0 && selectedFile != juce::File()) {
			presetBox.setSelectedId(selectedId,
						juce::dontSendNotification);
			currentPresetPath_ =
				selectedFile.getFullPathName();

			int rc = fxpw_load_preset(
				engine,
				currentPresetPath_.toWideCharPointer());
			bool restoredSession =
				savedSession.valid &&
				juce::File(savedSession.presetPath) ==
					selectedFile;
			if (restoredSession)
				applyPresetSession(savedSession);

			audioControls.update(engine);
			equalizer.refreshValues();
			applyPowerState();

			fprintf(stderr,
				"saucefx: startup preset '%s' %s%s\n",
				presetBox.getText().toRawUTF8(),
				rc == 0 ? "[ok]" : "[FAILED]",
				restoredSession ? " (session restored)" : "");
		}
		addAndMakeVisible(presetBox);

		/* Physical sinks and their wired/Bluetooth ports are discovered
		 * by the bridge and refreshed while the app is running. */
		outputBox.setJustificationType(
			juce::Justification::centredLeft);
		outputBox.setWantsKeyboardFocus(true);
		refreshOutputs();
		outputBox.addListener(this);
		addAndMakeVisible(outputBox);

		/* visualizer (40,92,960,120) */
		addAndMakeVisible(meter);

		/* FxAudioControls rack */
		addAndMakeVisible(audioControls);

		/* FxEqualizer band panel */
		int eqBands = fxpw_get_num_eq_bands(engine);
		fprintf(stderr, "saucefx: EQ bands reported by engine at panel "
				"construction: %d\n",
			eqBands);
		equalizer.initBands(engine, eqBands);
		addAndMakeVisible(equalizer);

		for (int b = 0; b < equalizer.numBands(); b++)
			equalizer.sliderForBand(b).addListener(this);
		for (int e = 0; e < 5; e++)
			audioControls.sliderFor(e).addListener(this);

		/* engine session -> sliders + labels at first paint */
		audioControls.update(engine);
		equalizer.refreshValues();
		audioControls.showValues(true);

		applyPowerState();

		/* engine status line (Linux app diagnostic, kept
		 * unobtrusive in the theme colors) */
		status.setJustificationType(juce::Justification::centredLeft);
		status.setFont(FxTheme::get().smallFont(12.0f));
		status.setColour(juce::Label::textColourId, fxc(HintText));
		addAndMakeVisible(status);

		/* settings sheet (gear button in the title bar) */
		settings = std::make_unique<FxSettingsOverlay>();
		settings->onClose = [this] { hideSettings(); };
		settings->onThemeMode = [this](int mode) {
			if (onThemeChange)
				onThemeChange(mode, fx_accent_custom_,
					      fx_accent_argb_);
		};
		settings->onAccent = [this](juce::Colour c) {
			if (onThemeChange)
				onThemeChange(fx_theme_mode_, true,
					      c.getARGB());
		};
		settings->onResetAccent = [this] {
			if (onThemeChange)
				onThemeChange(fx_theme_mode_, false,
					      0xffe63462u);
		};
		settings->onImport = [this] { chooseImportPresets(); };
		settings->onSave = [this](const juce::String &name) {
			saveCustomPreset(name);
		};
		settings->onSaveSuggestion = [this] {
			return savePresetSuggestion();
		};
		settings->onSaveNameExists = [this](const juce::String &raw) {
			auto name = sanitizedPresetName(raw);
			return name.isNotEmpty() &&
			       userPresetDir()
				       .getChildFile(name + ".fac")
				       .existsAsFile();
		};
		settings->onExport = [this] { chooseExportPreset(); };
		settings->onQuit = [] {
			if (auto *app = juce::JUCEApplication::getInstance())
				app->systemRequestedQuit();
		};
		settings->onAutostart = [this](bool enabled) {
			if (!applyAutostartSetting(enabled)) {
				settings->setAutostart(!enabled);
				settings->setStatus(
					"Could not update the login item.",
					true);
				return;
			}

			fx_autostart_enabled_ = enabled;
			saveGuiSettings(fx_theme_mode_, fx_accent_custom_,
					fx_accent_argb_,
					fx_autostart_enabled_,
					fx_output_routed_);
			settings->setStatus(
				enabled
					? "SauceFX will start at login."
					: "SauceFX will not start at login.",
				false);
		};
		settings->setAutostart(fx_autostart_enabled_);
		addChildComponent(*settings);

		/* the panel paints a rounded-bottom background over the
		 * transparent ARGB window; non-opaque so the window's
		 * rounded fill is repainted behind the corners */
		setOpaque(false);
		/* original pro view: setSize(WIDTH, HEIGHT+20) after the
		 * visualizer is shown - the extra 20 px hold the status */
		setSize(1040, 511);
		startTimerHz(30);
	}

	/* the title-bar power button routes through here */
	void setPowerState(bool on)
	{
		fxpw_set_power(engine, on ? 1 : 0);
		applyPowerState();
		queuePresetSessionSave();
		logStatus(juce::String());
	}

	/* Flush the selected preset + live values. Called by the debounce
	 * timer, window close and the app's shutdown path. */
	void savePresetSessionNow()
	{
		if (engine == nullptr)
			return;

		int bands = fxpw_get_num_eq_bands(engine);
		juce::String text;
		text << "version=1\n";
		if (currentPresetPath_.isNotEmpty())
			text << "preset=" << currentPresetPath_ << "\n";
		text << "power=" << (fxpw_get_power(engine) ? "1" : "0")
		     << "\n";
		for (int e = 0; e < 5; e++)
			text << "effect" << e << "="
			     << juce::String::formatted(
					"%.6f",
					(double)fxpw_get_effect_value(engine, e))
			     << "\n";
		text << "bands=" << bands << "\n";
		for (int b = 0; b < bands; b++) {
			text << "eq" << b << "freq="
			     << juce::String::formatted(
					"%.6f",
					(double)fxpw_get_eq_band_freq(engine, b))
			     << "\n";
			text << "eq" << b << "gain="
			     << juce::String::formatted(
					"%.6f",
					(double)fxpw_get_eq_band_boostcut(engine,
									  b))
			     << "\n";
		}

		stateDir().createDirectory();
		if (!presetSessionFile().replaceWithText(
			    text, false, false, "\n"))
			fprintf(stderr,
				"saucefx: could not write %s\n",
				presetSessionFile()
					.getFullPathName()
					.toRawUTF8());
		sessionDirty_ = false;
	}

	/* the title-bar gear button routes through here */
	void toggleSettings()
	{
		if (settings == nullptr)
			return;
		if (settings->isVisible())
			hideSettings();
		else
			showSettings();
	}

	void showSettings()
	{
		if (settings == nullptr)
			return;
		settings->setBounds(getLocalBounds());
		settings->refreshFromTheme();
		settings->showAnimated();
		settings->toFront(true);
		settings->grabKeyboardFocus();
		if (onSettingsVisibilityChanged)
			onSettingsVisibilityChanged(true);
	}

	void hideSettings()
	{
		if (settings == nullptr || !settings->isOpen())
			return;
		settings->hideAnimated();
		if (onSettingsVisibilityChanged)
			onSettingsVisibilityChanged(false);
	}

	bool isSettingsVisible() const
	{
		return settings != nullptr && settings->isOpen();
	}

	/* ---- web interface bridge ------------------------------------- */

	std::array<float, DFXP_BAND_COUNT> webSpectrum() const
	{
		return lastSpectrumBands_;
	}

	juce::String webAccentHex() const
	{
		if (!fx_accent_custom_)
			return "#f2486f";
		return "#" +
		       juce::String::toHexString(
			       (int)(fx_accent_argb_ & 0xffffffu))
			       .paddedLeft('0', 6);
	}

	juce::var createWebState()
	{
		auto *state = new juce::DynamicObject();
		state->setProperty("power", fxpw_get_power(engine) != 0);
		auto outputs = createWebOutputs();
		state->setProperty("output", outputs["output"]);
		state->setProperty("outputs", outputs["outputs"]);

		juce::Array<juce::var> presets;
		int selectedId = presetBox.getSelectedId();
		juce::String selectedPath = currentPresetPath_;
		juce::String selectedName;
		juce::String selectedGroup = "factory";
		for (int i = 0; i < presetFiles.size(); ++i) {
			juce::File file(presetFiles[i]);
			const bool custom =
				file.getParentDirectory() == userPresetDir();
			auto *entry = new juce::DynamicObject();
			entry->setProperty("id", file.getFullPathName());
			entry->setProperty("name", presetBox.getItemText(i));
			entry->setProperty("group",
					   custom ? "custom" : "factory");
			presets.add(juce::var(entry));
			if (selectedId == i + 1) {
				selectedPath = file.getFullPathName();
				selectedName = presetBox.getItemText(i);
				selectedGroup = custom ? "custom" : "factory";
			}
		}

		if (selectedName.isEmpty() && !presetFiles.isEmpty()) {
			int index = juce::jlimit(0, presetFiles.size() - 1,
						 selectedId - 1);
			juce::File file(presetFiles[index]);
			selectedPath = file.getFullPathName();
			selectedName = presetBox.getItemText(index);
			selectedGroup =
				file.getParentDirectory() == userPresetDir()
					? "custom"
					: "factory";
		}

		state->setProperty("presets", presets);
		state->setProperty("presetId", selectedPath);
		state->setProperty("presetName", selectedName);
		state->setProperty("presetGroup", selectedGroup);

		juce::Array<juce::var> effects;
		for (int effect = 0; effect < 5; ++effect)
			effects.add(
				(double)fxpw_get_effect_value(engine, effect));
		state->setProperty("effects", effects);

		juce::Array<juce::var> eq;
		const int bands = fxpw_get_num_eq_bands(engine);
		for (int band = 0; band < bands; ++band)
			eq.add((double)fxpw_get_eq_band_boostcut(engine,
								 band));
		state->setProperty("eq", eq);

		state->setProperty("autostart", fx_autostart_enabled_);
		state->setProperty("theme",
				   fx_theme_mode_ == ThemeLight ? "light"
								 : "dark");
		state->setProperty("accent", webAccentHex());
		state->setProperty("customAccent", fx_accent_custom_);
		state->setProperty("version", SAUCEFX_VERSION);
		return juce::var(state);
	}

	/* Persist processing mode separately from the remembered physical device. */
	void saveWebSettings()
	{
		fx_output_routed_ = fxpw_is_routed_default(engine) != 0;
		saveGuiSettings(fx_theme_mode_, fx_accent_custom_,
				fx_accent_argb_, fx_autostart_enabled_,
				fx_output_routed_);
	}

	void webSetPower(bool on)
	{
		setPowerState(on);
	}

	void webSetPreset(const juce::String &path)
	{
		juce::File wanted(path);
		if (!wanted.existsAsFile()) {
			if (onWebNotification)
				onWebNotification("Preset file is no longer available.",
						  true);
			return;
		}

		for (int i = 0; i < presetFiles.size(); ++i) {
			if (juce::File(presetFiles[i]) != wanted)
				continue;
			presetBox.setSelectedId(i + 1,
						juce::dontSendNotification);
			loadPresetPath(wanted);
			return;
		}
	}

	void webSetOutput(const juce::String &id)
	{
		refreshOutputs();
		auto choice = std::find_if(outputChoices_.begin(), outputChoices_.end(),
			[&](const auto &item) { return item.id == id && item.available; });
		if (choice == outputChoices_.end()) {
			if (onWebNotification)
				onWebNotification("That audio output is no longer connected.", true);
			return;
		}
		const auto selected = *choice;
		FXPWOutputState previous{};
		fxpw_get_output_state(engine, &previous);
		int rc = 0;
		if (id == "system") {
			rc = fxpw_route_default(engine, 0);
		} else {
			rc = fxpw_set_output(engine, selected.name.toRawUTF8(), selected.port.toRawUTF8());
			if (rc == 0)
				rc = fxpw_route_default(engine, 1);
		}
		if (rc == 0) {
			if (id != "system") {
				fx_output_device_ = selected.name;
				fx_output_port_ = selected.port;
			}
			saveWebSettings();
		} else {
			fxpw_set_output(engine, previous.preferred_name, previous.preferred_port);
			if (onWebNotification)
				onWebNotification("Could not change audio output.", true);
		}
		refreshOutputs();
	}

	juce::var createWebOutputs()
	{
		refreshOutputs();
		return outputState_;
	}

	void webSetEffect(int index, double value)
	{
		if (index < 0 || index >= 5)
			return;
		fxpw_set_effect_value(engine, index, (float)value);
		queuePresetSessionSave();
	}

	void webSetEqBand(int index, double value)
	{
		const int bands = fxpw_get_num_eq_bands(engine);
		if (index < 0 || index >= bands)
			return;
		fxpw_set_eq_band(engine, index,
				 fxpw_get_eq_band_freq(engine, index),
				 (float)value);
		queuePresetSessionSave();
	}

	void webSetEqValues(const juce::var &values)
	{
		const auto *array = values.getArray();
		if (array == nullptr)
			return;
		const int bands = fxpw_get_num_eq_bands(engine);
		const int count = juce::jmin(bands, array->size());
		for (int band = 0; band < count; ++band)
			fxpw_set_eq_band(engine, band,
					 fxpw_get_eq_band_freq(engine, band),
					 (float)(double)(*array)[band]);
		queuePresetSessionSave();
	}

	void webSavePreset(const juce::String &rawName)
	{
		auto name = sanitizedPresetName(rawName);
		if (name.isEmpty()) {
			if (onWebNotification)
				onWebNotification("Enter a valid preset name.", true);
			return;
		}
		/* The web dialog already validates duplicates. Always go through
		 * the unique-name path so neither a factory nor an existing user
		 * file can be overwritten by a stale request. */
		writeCustomPreset(uniqueCustomPresetName(name));
	}

	void webImportPresets() { chooseImportPresets(); }
	void webExportPreset() { chooseExportPreset(); }

	bool webSetAutostart(bool enabled)
	{
		if (!applyAutostartSetting(enabled)) {
			if (onWebNotification)
				onWebNotification("Could not update the login item.",
						  true);
			return false;
		}
		fx_autostart_enabled_ = enabled;
		saveWebSettings();
		return true;
	}

	void webSetThemeMode(const juce::String &mode)
	{
		fx_theme_mode_ =
			mode.equalsIgnoreCase("light") ? ThemeLight : ThemeDark;
		saveWebSettings();
	}

	void webSetAccent(const juce::String &accent)
	{
		auto value = accent.trim();
		if (value.startsWithChar('#'))
			value = value.substring(1);
		if (value.length() != 6 ||
		    !value.containsOnly("0123456789abcdefABCDEF")) {
			if (onWebNotification)
				onWebNotification("Invalid accent color.", true);
			return;
		}
		fx_accent_custom_ = true;
		fx_accent_argb_ = 0xff000000u |
				  (value.getHexValue32() & 0xffffffu);
		saveWebSettings();
	}

	/* called by the window after FxTheme/LnF have been re-applied */
	void refreshTheme()
	{
		audioControls.refreshTheme();
		equalizer.refreshTheme();
		status.setColour(juce::Label::textColourId, fxc(HintText));
		if (settings != nullptr)
			settings->refreshFromTheme();
		presetBox.repaint();
		outputBox.repaint();
		repaint();
	}

	void paint(juce::Graphics &g) override
	{
		/* FxProView::paint - rounded bottom corners like the
		 * original window (the top corners belong to the title bar) */
		juce::Path p;
		p.addRoundedRectangle(
			0.0f, 0.0f, (float)getWidth(), (float)getHeight(),
			(float)FX_WINDOW_CORNER_RADIUS,
			(float)FX_WINDOW_CORNER_RADIUS, false, false, true,
			true);
		g.setFillType(juce::FillType(fxc(WindowBackground)));
		g.fillPath(p);

		/* visualizer always shown: offset = 120 + 20 */
		g.setFillType(juce::FillType(fxc(PanelBackground, 0.2f)));
		g.fillRoundedRectangle(20, 16, 1000, 347 + 140, 8);
	}

	void resized() override
	{
		static int printed_once = 0;
		if (!printed_once++) {
			fprintf(stderr, "saucefx: content sized %d x %d\n",
				getWidth(), getHeight());
			fflush(stderr);
		}

		/* FxProView::resized with the visualizer visible */
		presetBox.setBounds(40, 32, 470, 40);
		outputBox.setBounds(530, 32, 470, 40);

		/* meter at (40, preset bottom + 20) size 960x120 */
		meter.setBounds(40, presetBox.getBottom() + 20,
				FxVisualizer::WIDTH, FxVisualizer::HEIGHT);

		audioControls.setBounds(40, 88 + 140, 168, 257);
		equalizer.setBounds(audioControls.getRight() + 16, 88 + 140,
				    776, 257);

		status.setBounds(40, getHeight() - 26, 640, 18);
		if (settings != nullptr)
			settings->setBounds(getLocalBounds());
	}

private:
	FXPW *engine;
	std::array<float, DFXP_BAND_COUNT> lastSpectrumBands_{};
	juce::ComboBox presetBox;
	juce::StringArray presetFiles;
	juce::String currentPresetPath_;
	bool sessionDirty_ = false;
	double sessionDirtyAtMs_ = 0.0;
	juce::ComboBox outputBox;
	struct OutputChoice {
		juce::String id, label, detail, name, port;
		bool available = true;
	};
	std::vector<OutputChoice> outputChoices_;
	juce::var outputState_;
	juce::String outputJson_;
	FxEffects audioControls;
	FxEqualizer equalizer;
	FxVisualizer meter;
	juce::Label status;
	std::unique_ptr<FxSettingsOverlay> settings;
	std::unique_ptr<juce::FileChooser> importChooser;
	std::unique_ptr<juce::FileChooser> exportChooser;

	static juce::String outputId(const juce::String &name, const juce::String &port)
	{
		return "device:" + juce::Base64::toBase64(name) + ":" + juce::Base64::toBase64(port);
	}

	void refreshOutputs()
	{
		std::vector<FXPWOutput> devices(fxpw_get_outputs(engine, nullptr, 0) + 4);
		const auto count = fxpw_get_outputs(engine, devices.data(), devices.size());
		devices.resize(std::min(count, devices.size()));
		FXPWOutputState state{};
		fxpw_get_output_state(engine, &state);
		juce::String activeLabel;
		juce::String preferredPort = juce::String::fromUTF8(state.preferred_port);
		for (const auto &device : devices) {
			if (strcmp(device.name, state.active_name) != 0 ||
			    (device.port[0] != '\0' && strcmp(device.port, state.active_port) != 0))
				continue;
			activeLabel = juce::String::fromUTF8(device.description);
			if (device.port_description[0] != '\0')
				activeLabel = juce::String::fromUTF8(device.port_description) + " — " + activeLabel;
			if (preferredPort.isEmpty() && strcmp(device.name, state.preferred_name) == 0)
				preferredPort = juce::String::fromUTF8(device.port);
		}
		if (activeLabel.isEmpty())
			activeLabel = state.active_name[0] != '\0'
				? juce::String::fromUTF8(state.active_name) : "Waiting for an audio device";

		std::vector<OutputChoice> choices{
			{"system", "System audio", "Unprocessed system output", {}, {}, true},
			{"saucefx", "Automatic (SauceFX)", activeLabel, {}, {}, true},
		};
		for (const auto &device : devices) {
			auto name = juce::String::fromUTF8(device.name);
			auto port = juce::String::fromUTF8(device.port);
			auto label = juce::String::fromUTF8(device.description);
			if (device.port_description[0] != '\0')
				label = juce::String::fromUTF8(device.port_description) + " — " + label;
			choices.push_back({outputId(name, port), label,
				device.available ? "Processed by SauceFX" : "Not connected",
				name, port, device.available != 0});
		}
		juce::String selected = state.routed ? "saucefx" : "system";
		if (state.routed && state.preferred_name[0] != '\0') {
			auto name = juce::String::fromUTF8(state.preferred_name);
			selected = outputId(name, preferredPort);
			auto preferred = std::find_if(choices.begin(), choices.end(),
				[&](const auto &choice) { return choice.id == selected; });
			if (preferred == choices.end()) {
				auto previous = std::find_if(outputChoices_.begin(), outputChoices_.end(),
					[&](const auto &choice) { return choice.id == selected; });
				choices.push_back({selected,
					previous != outputChoices_.end() ? previous->label : name,
					"Disconnected · " + activeLabel, name, preferredPort, false});
			} else if (!preferred->available)
				preferred->detail = "Disconnected · " + activeLabel;
		}
		auto *object = new juce::DynamicObject();
		object->setProperty("output", selected);
		juce::Array<juce::var> options;
		for (const auto &choice : choices) {
			auto *entry = new juce::DynamicObject();
			entry->setProperty("id", choice.id);
			entry->setProperty("label", choice.label);
			entry->setProperty("detail", choice.detail);
			entry->setProperty("available", choice.available);
			options.add(juce::var(entry));
		}
		object->setProperty("outputs", options);
		juce::var next(object);
		auto json = juce::JSON::toString(next, true);
		if (json == outputJson_)
			return;
		outputJson_ = json;
		outputState_ = next;
		outputChoices_ = std::move(choices);
		outputBox.clear(juce::dontSendNotification);
		for (size_t i = 0; i < outputChoices_.size(); ++i) {
			const auto &choice = outputChoices_[i];
			int id = static_cast<int>(i) + 1;
			outputBox.addItem(choice.label, id);
			outputBox.setItemEnabled(id, choice.available);
			if (choice.id == selected)
				outputBox.setSelectedId(id, juce::dontSendNotification);
		}
	}

	/* ---- preset library ---- */

	void rebuildPresetList(const juce::File &selectAfter = {})
	{
		auto previous = juce::File();
		int oldId = presetBox.getSelectedId();
		if (oldId > 0 && oldId <= presetFiles.size())
			previous = juce::File(presetFiles[oldId - 1]);

		presetBox.clear(juce::dontSendNotification);
		presetFiles.clear();

		auto presets = findPresets();
		int id = 1;
		for (auto &f : presets) {
			char name[128];
			if (fxpw_preset_name(
				    engine, f.getFullPathName().toWideCharPointer(),
				    name, sizeof(name)) != 0)
				snprintf(name, sizeof(name), "%s",
					 f.getFileNameWithoutExtension()
						 .toRawUTF8());
			presetBox.addItem(juce::String(name), id);
			presetFiles.add(f.getFullPathName());
			id++;
		}

		juce::File wanted =
			selectAfter.existsAsFile() ? selectAfter : previous;
		int selected = 0;
		if (wanted.existsAsFile()) {
			for (int i = 0; i < presetFiles.size(); i++) {
				if (juce::File(presetFiles[i]) == wanted) {
					selected = i + 1;
					break;
				}
			}
		}
		if (selected == 0 && presetBox.getNumItems() > 0)
			selected = 1;
		if (selected > 0)
			presetBox.setSelectedId(selected,
						juce::dontSendNotification);
	}

	void loadPresetPath(const juce::File &path,
			    bool remember = true)
	{
		int rc = fxpw_load_preset(engine,
					  path.getFullPathName()
						  .toWideCharPointer());
		currentPresetPath_ = path.getFullPathName();
		logStatus(juce::String("loaded ") + path.getFileName() +
			  (rc == 0 ? " [ok]" : " [FAILED]"));
		audioControls.update(engine);
		equalizer.refreshValues();
		applyPowerState();
		if (remember)
			queuePresetSessionSave();
	}

	void applyPresetSession(const PresetSession &s)
	{
		for (int e = 0; e < 5; e++)
			fxpw_set_effect_value(engine, e, s.effects[e]);

		int bands = juce::jmin(fxpw_get_num_eq_bands(engine),
				       s.eqBands);
		for (int b = 0; b < bands; b++)
			fxpw_set_eq_band(engine, b, s.eqFreq[b],
					 s.eqGain[b]);

		fxpw_set_power(engine, s.power ? 1 : 0);
	}

	void queuePresetSessionSave()
	{
		sessionDirty_ = true;
		sessionDirtyAtMs_ =
			juce::Time::getMillisecondCounterHiRes();
	}

	bool loadSelectedPreset()
	{
		int id = presetBox.getSelectedId();
		if (id <= 0 || id > presetFiles.size())
			return false;
		loadPresetPath(juce::File(presetFiles[id - 1]));
		return true;
	}

	juce::String savePresetSuggestion() const
	{
		auto current = presetBox.getText().trim();
		if (current.isEmpty())
			return "My preset";

		auto currentFile = juce::File(currentPresetPath_);
		bool inUserLibrary =
			currentFile.getParentDirectory() == userPresetDir();
		return inUserLibrary ? current : current + " custom";
	}

	juce::String sanitizedPresetName(const juce::String &raw) const
	{
		auto name = raw.trim();
		if (name.isEmpty())
			return {};

		/* Keep user-entered names safe as .fac filenames. This also
		 * prevents a name from escaping the user preset directory. */
		name = name.replaceCharacters("/\\:*?\"<>|", "_________");
		name = name.replaceCharacters("\t\r\n", "   ");
		for (juce::juce_wchar c = 1; c < 32; ++c)
			name = name.replace(juce::String::charToString(c), " ");
		name = name.substring(0, 80).trim();

		if (name == "." || name == "..")
			return {};
		return name;
	}

	juce::String uniqueCustomPresetName(const juce::String &base)
	{
		auto dir = userPresetDir();
		auto isTaken = [this, &dir](const juce::String &name) {
			if (dir.getChildFile(name + ".fac").existsAsFile())
				return true;
			for (int i = 1; i <= presetBox.getNumItems(); ++i)
				if (presetBox.getItemText(i - 1)
					    .equalsIgnoreCase(name))
					return true;
			return false;
		};

		if (!isTaken(base))
			return base;

		for (int n = 2; n < 1000; ++n) {
			auto candidate =
				base + " (" + juce::String(n) + ")";
			if (!isTaken(candidate))
				return candidate;
		}

		return base + " " +
		       juce::String::toHexString(
			       juce::Time::currentTimeMillis());
	}

	void saveCustomPreset(const juce::String &rawName)
	{
		auto name = sanitizedPresetName(rawName);
		if (name.isEmpty()) {
			if (settings != nullptr)
				settings->setStatus(
					"Enter a valid preset name.", true);
			return;
		}

		auto dir = userPresetDir();
		if (!dir.createDirectory()) {
			if (settings != nullptr)
				settings->setStatus(
					"Could not create the preset folder.",
					true);
			return;
		}

		/* The prompt has already confirmed replacing an exact
		 * custom preset. Different display names are non-destructive
		 * and get a unique suffix instead. */
		auto target = dir.getChildFile(name + ".fac");
		writeCustomPreset(target.existsAsFile()
					  ? name
					  : uniqueCustomPresetName(name));
	}

	bool writeCustomPreset(const juce::String &name)
	{
		auto dir = userPresetDir();
		if (!dir.createDirectory()) {
			auto msg = juce::String(
				"Could not create the preset folder.");
			if (settings != nullptr)
				settings->setStatus(msg, true);
			if (onWebNotification)
				onWebNotification(msg, true);
			return false;
		}

		auto target = dir.getChildFile(name + ".fac");
		bool ok = fxpw_save_preset(
				  engine, name.toWideCharPointer(),
				  dir.getFullPathName()
					  .toWideCharPointer()) == 0;
		if (ok)
			ok = target.existsAsFile() && target.getSize() > 0;

		if (!ok) {
			auto msg = "Could not save preset \"" + name + "\".";
			if (settings != nullptr)
				settings->setStatus(msg, true);
			if (onWebNotification)
				onWebNotification(msg, true);
			logStatus(msg);
			fprintf(stderr,
				"saucefx: save preset failed: %s\n",
				target.getFullPathName().toRawUTF8());
			return false;
		}

		rebuildPresetList(target);
		loadSelectedPreset();

		auto msg = "Saved preset \"" + name + "\".";
		if (settings != nullptr)
			settings->setStatus(msg, false);
		if (onWebStateChanged)
			onWebStateChanged();
		if (onWebNotification)
			onWebNotification(msg, false);
		logStatus(msg);
		fprintf(stderr, "saucefx: saved custom preset %s\n",
			target.getFullPathName().toRawUTF8());
		return true;
	}

	void chooseImportPresets()
	{
		auto start = userPresetDir().isDirectory()
				     ? userPresetDir()
				     : juce::File::getSpecialLocation(
					       juce::File::
						       userHomeDirectory);
		importChooser = std::make_unique<juce::FileChooser>(
			"Import presets", start, "*.fac",
			/* useNativeDialogBox */ false);
		auto safe = juce::Component::SafePointer<SauceFXPanel>(this);
		importChooser->launchAsync(
			juce::FileBrowserComponent::openMode |
				juce::FileBrowserComponent::canSelectFiles |
				juce::FileBrowserComponent::
					canSelectMultipleItems,
			[safe](const juce::FileChooser &fc) {
				if (safe != nullptr)
					safe->importPresets(fc.getResults());
			});
	}

	void importPresets(const juce::Array<juce::File> &files)
	{
		if (files.isEmpty())
			return;

		auto dir = userPresetDir();
		dir.createDirectory();

		juce::StringArray existing;
		for (int i = 1; i <= presetBox.getNumItems(); i++)
			existing.add(presetBox.getItemText(i - 1)
					     .trim()
					     .toLowerCase());

		int imported = 0, skipped = 0, failed = 0;
		juce::File last;

		for (auto &src : files) {
			char raw[256];
			if (fxpw_preset_name(
				    engine, src.getFullPathName().toWideCharPointer(),
				    raw, sizeof(raw)) != 0) {
				failed++;
				continue;
			}

			juce::String name = juce::String(raw).trim();
			if (name.isEmpty())
				name = src.getFileNameWithoutExtension();
			if (existing.contains(name.toLowerCase())) {
				skipped++;
				continue;
			}

			juce::String safeName =
				name.replaceCharacters("/\\:*?\"<>|", "_________");
			if (safeName.isEmpty())
				safeName = "Preset";

			auto target = dir.getChildFile(safeName + ".fac");
			int n = 2;
			while (target.existsAsFile())
				target = dir.getChildFile(
					safeName + " " + juce::String(n++) +
					".fac");

			if (!src.copyFileTo(target)) {
				failed++;
				continue;
			}
			existing.add(name.toLowerCase());
			last = target;
			imported++;
		}

		if (imported > 0) {
			rebuildPresetList(last);
			loadSelectedPreset();
		}

		juce::String msg;
		if (imported > 0)
			msg << "Imported " << imported
			    << (imported == 1 ? " preset" : " presets");
		if (skipped > 0)
			msg << (msg.isEmpty() ? "" : ", ") << "skipped "
			    << skipped
			    << (skipped == 1 ? " duplicate" : " duplicates");
		if (failed > 0)
			msg << (msg.isEmpty() ? "" : ", ") << failed
			    << " not readable";
		if (msg.isEmpty())
			msg = "No presets imported.";

		if (settings != nullptr)
			settings->setStatus(msg, imported == 0);
		if (imported > 0 && onWebStateChanged)
			onWebStateChanged();
		if (onWebNotification)
			onWebNotification(msg, imported == 0);
		logStatus(msg);
	}

	void chooseExportPreset()
	{
		auto current = presetBox.getText().trim();
		if (current.isEmpty())
			current = "SauceFX";
		auto start = juce::File::getSpecialLocation(
				     juce::File::userHomeDirectory)
				     .getChildFile(current + ".fac");

		exportChooser = std::make_unique<juce::FileChooser>(
			"Export current preset", start, "*.fac",
			/* useNativeDialogBox */ false);
		auto safe = juce::Component::SafePointer<SauceFXPanel>(this);
		exportChooser->launchAsync(
			juce::FileBrowserComponent::saveMode |
				juce::FileBrowserComponent::canSelectFiles |
				juce::FileBrowserComponent::warnAboutOverwriting,
			[safe](const juce::FileChooser &fc) {
				if (safe == nullptr)
					return;
				auto target = fc.getResult();
				if (target == juce::File())
					return;
				if (!target.getFileExtension().equalsIgnoreCase(
					    ".fac"))
					target = target.withFileExtension(
						".fac");
				safe->exportCurrentPreset(target);
			});
	}

	void exportCurrentPreset(const juce::File &target)
	{
		auto name = target.getFileNameWithoutExtension();
		auto dir = target.getParentDirectory();
		bool ok = fxpw_save_preset(
				  engine, name.toWideCharPointer(),
				  dir.getFullPathName().toWideCharPointer()) == 0;
		if (ok)
			ok = target.existsAsFile();

		juce::String msg =
			ok ? "Exported " + target.getFileName()
			   : "Export failed: " + target.getFullPathName();
		if (settings != nullptr)
			settings->setStatus(msg, !ok);
		if (onWebNotification)
			onWebNotification(msg, !ok);
		logStatus(msg);
	}

	/* ---- engine control (message thread) ---- */

	void applyPowerState()
	{
		bool on = fxpw_get_power(engine) != 0;
		audioControls.setEnabled(on);
		equalizer.setEnabled(on);
		presetBox.setEnabled(on);
		outputBox.setEnabled(true);
		audioControls.showValues(true);
		equalizer.repaint();
	}

	void comboBoxChanged(juce::ComboBox *box) override
	{
		if (box == &presetBox) {
			int ident = box->getSelectedId();
			if (ident <= 0 || ident > presetFiles.size())
				return;

			loadPresetPath(juce::File(presetFiles[ident - 1]));
			return;
		}

		if (box == &outputBox) {
			int ident = box->getSelectedId();
			if (ident > 0 && static_cast<size_t>(ident) <= outputChoices_.size()) {
				auto id = outputChoices_[static_cast<size_t>(ident - 1)].id;
				webSetOutput(id);
			}
			return;
		}
	}

	void sliderValueChanged(juce::Slider *s) override
	{
		for (int e = 0; e < 5; e++) {
			if (s != &audioControls.sliderFor(e))
				continue;
			/* UI value is 0..10 raw (DfxDsp scales x10) */
			fxpw_set_effect_value(engine, e, (float)s->getValue());
			audioControls.positionValueLabel(e);
			queuePresetSessionSave();
			return;
		}

		for (int b = 0; b < equalizer.numBands(); b++) {
			if (s != &equalizer.sliderForBand(b))
				continue;
			/* gain push; keep the band's own frequency (the
			 * engine clamps and persists the band) */
			fxpw_set_eq_band(engine, b,
					 fxpw_get_eq_band_freq(engine, b),
					 (float)s->getValue());
			equalizer.sliderForBand(b).updateLabel();
			queuePresetSessionSave();
			return;
		}
	}

	void logStatus(const juce::String extra)
	{
		long long frames = 0, dropped = 0, underruns = 0;
		int rate = 0, ch = 0, state = 0, ringf = 0;
		fxpw_get_stats(engine, &frames, &dropped, &underruns, &rate,
			       &ch, &state, &ringf);

		static const char *stateNames[5] = { "ENGINE ERROR", "waiting",
			"connecting", "prefilling", "running" };
		double secs = rate > 0 ? frames / (double)rate : 0.0;

		status.setText(juce::String::formatted(
				       "%s | %g kHz %d ch | %.1f s processed | "
				       "dropped %lld | underruns %lld | ring %d fr | %s",
				       stateNames[juce::jlimit(0, 4, state)],
				       rate / 1000.0, ch, secs, dropped,
				       underruns, ringf, extra),
			       juce::dontSendNotification);
	}

	void timerCallback() override
	{
		float bands[DFXP_BAND_COUNT];
		fxpw_get_spectrum(engine, bands, DFXP_BAND_COUNT);
		std::copy(bands, bands + DFXP_BAND_COUNT,
			  lastSpectrumBands_.begin());

		static int printed = 0;
		if (printed++ == 3) {
			fprintf(stderr, "fxpwspec:");
			for (int i = 0; i < DFXP_BAND_COUNT; i++)
				fprintf(stderr, " %g", (double)bands[i]);
			fprintf(stderr, "\n");
		}

		meter.push(bands, DFXP_BAND_COUNT);

		if (sessionDirty_ &&
		    juce::Time::getMillisecondCounterHiRes() -
				    sessionDirtyAtMs_ >=
			    250.0)
			savePresetSessionNow();

		static int tick = 0;
		if ((tick++ % 8) == 0) {
			logStatus(juce::String());
			refreshOutputs();
		}
	}

	JUCE_DECLARE_NON_COPYABLE(SauceFXPanel)
};

/* ================================================================== */
/* React/Base UI host. The native panel above remains the engine and     */
/* preset backend, but is never shown in this mode.                     */

class WebInterface : public juce::Component, private juce::Timer
{
public:
	std::function<void()> onMinimize;
	std::function<void()> onClose;
	std::function<void()> onQuit;

	explicit WebInterface(SauceFXPanel &panelIn)
		: panel(panelIn), root(webRootDir())
	{
		setOpaque(true);
		setSize(1040, 568);

		const juce::String theme =
			fx_theme_mode_ == ThemeLight ? "light" : "dark";
		const juce::String accent = panel.webAccentHex();

		auto options = juce::WebBrowserComponent::Options{}
				       .withNativeIntegrationEnabled()
				       .withKeepPageLoadedWhenBrowserIsHidden()
				       .withInitialisationData("saucefxTheme",
							       theme)
				       .withInitialisationData("saucefxAccent",
							       accent)
				       .withEventListener(
					       "saucefxCommand",
					       [this](const juce::var &message) {
						       handleCommand(message);
					       })
				       .withUserScript(bridgeScript())
				       .withResourceProvider(
					       [this](const juce::String &path) {
						       return serve(path);
					       });

		browser = std::make_unique<juce::WebBrowserComponent>(options);
		addAndMakeVisible(*browser);
		/* The component is already 1040x568 here, so adding the
		 * child does not trigger a size change. Give the native
		 * web view its real viewport explicitly. */
		browser->setBounds(getLocalBounds());

		errorLabel.setJustificationType(
			juce::Justification::centred);
		errorLabel.setColour(juce::Label::textColourId,
				     juce::Colour(0xffe9e2da));
		errorLabel.setColour(juce::Label::backgroundColourId,
				     juce::Colour(0xff111214));
		errorLabel.setFont(juce::Font(juce::FontOptions(15.0f)));
		errorLabel.setText(
			"Web interface files were not found.\n"
			"Build the frontend or run the CMake build again.",
			juce::dontSendNotification);
		addChildComponent(errorLabel);

		panel.onWebStateChanged = [this] { pushState(); };
		panel.onWebNotification = [this](const juce::String &message,
						 bool error) {
			juce::ignoreUnused(error);
			pushToast(message);
		};

		const auto devUrl = juce::SystemStats::getEnvironmentVariable(
			"SAUCEFX_WEB_DEV", {});
		if (devUrl.isNotEmpty()) {
			fprintf(stderr, "saucefx: web dev URL %s\n",
				devUrl.toRawUTF8());
			browser->goToURL(devUrl);
		} else if (root != juce::File()) {
			fprintf(stderr, "saucefx: web root %s\n",
				root.getFullPathName().toRawUTF8());
			browser->goToURL(
				juce::WebBrowserComponent::getResourceProviderRoot());
		} else {
			fprintf(stderr, "saucefx: web assets not found\n");
			errorLabel.setVisible(true);
			errorLabel.toFront(false);
		}

		startTimerHz(30);
	}

	~WebInterface() override
	{
		stopTimer();
		panel.onWebStateChanged = nullptr;
		panel.onWebNotification = nullptr;
	}

	void paint(juce::Graphics &g) override
	{
		g.fillAll(fx_theme_mode_ == ThemeLight
				  ? juce::Colour(0xfff6f1ea)
				  : juce::Colour(0xff111214));
	}

	void resized() override
	{
		auto bounds = getLocalBounds();
		if (browser != nullptr)
			browser->setBounds(bounds);
		errorLabel.setBounds(bounds);
	}

	void refreshState()
	{
		pushState();
		pushOutputs();
		pushSpectrum();
	}

private:
	static const char *bridgeScript()
	{
		return R"JS(
(function () {
  var juce = window.__JUCE__;
  if (!juce || !juce.backend) return;
  var backend = juce.backend;
  function subscribe(eventId) {
    return function (fn) {
      var handle = backend.addEventListener(eventId, fn);
      return function () { backend.removeEventListener(handle); };
    };
  }
  window.saucefxHost = {
    command: function (type, payload) {
      backend.emitEvent('saucefxCommand', {
        type: type,
        payload: payload === undefined ? null : payload
      });
    },
    onState: subscribe('saucefxState'),
    onOutputs: subscribe('saucefxOutputs'),
    onSpectrum: subscribe('saucefxSpectrum'),
    onToast: subscribe('saucefxToast')
  };
})();
)JS";
	}

	static juce::var property(const juce::var &object,
				  const char *name)
	{
		if (auto *dynamic = object.getDynamicObject())
			return dynamic->getProperty(juce::Identifier(name));
		return {};
	}

	static bool boolValue(const juce::var &value, bool fallback = false)
	{
		if (value.isBool())
			return (bool)value;
		if (value.isInt() || value.isInt64() || value.isDouble())
			return (double)value != 0.0;
		return fallback;
	}

	static double doubleValue(const juce::var &value,
				  double fallback = 0.0)
	{
		if (value.isDouble() || value.isInt() || value.isInt64())
			return (double)value;
		if (value.isString())
			return value.toString().getDoubleValue();
		return fallback;
	}

	static juce::String stringValue(const juce::var &value)
	{
		return value.isString() ? value.toString() : juce::String();
	}

	static juce::String mimeTypeFor(const juce::File &file)
	{
		auto extension = file.getFileExtension().toLowerCase();
		if (extension == ".html" || extension == ".htm")
			return "text/html";
		if (extension == ".js" || extension == ".mjs")
			return "text/javascript";
		if (extension == ".css")
			return "text/css";
		if (extension == ".json" || extension == ".map")
			return "application/json";
		if (extension == ".svg")
			return "image/svg+xml";
		if (extension == ".png")
			return "image/png";
		if (extension == ".jpg" || extension == ".jpeg")
			return "image/jpeg";
		if (extension == ".webp")
			return "image/webp";
		if (extension == ".woff")
			return "font/woff";
		if (extension == ".woff2")
			return "font/woff2";
		if (extension == ".ttf")
			return "font/ttf";
		if (extension == ".otf")
			return "font/otf";
		return "application/octet-stream";
	}

	std::optional<juce::WebBrowserComponent::Resource>
	serve(const juce::String &requestPath)
	{
		const bool log =
			juce::SystemStats::getEnvironmentVariable(
				"SAUCEFX_WEB_LOG", {})
				.isNotEmpty();
		if (log)
			fprintf(stderr, "saucefx: web resource '%s'\n",
				requestPath.toRawUTF8());
		if (root == juce::File())
			return std::nullopt;

		auto path =
			requestPath
				.upToFirstOccurrenceOf("?", false, false)
				.upToFirstOccurrenceOf("#", false, false);
		path = juce::URL::removeEscapeChars(path);
		if (path.isEmpty() || path == "/")
			path = "/index.html";
		if (path.contains(".."))
			return std::nullopt;

		while (path.startsWithChar('/'))
			path = path.substring(1);

		auto file = root.getChildFile(path);
		if (!file.isAChildOf(root) || !file.existsAsFile())
			return std::nullopt;

		juce::MemoryBlock block;
		if (!file.loadFileAsData(block))
			return std::nullopt;

		std::vector<std::byte> data(block.getSize());
		if (block.getSize() > 0)
			std::memcpy(data.data(), block.getData(), block.getSize());

		return juce::WebBrowserComponent::Resource{
			std::move(data), mimeTypeFor(file)};
	}

	void handleCommand(const juce::var &message)
	{
		auto type = property(message, "type").toString();
		auto payload = property(message, "payload");

		if (type == "ready") {
			ready = true;
			fprintf(stderr, "saucefx: web interface ready\n");
			refreshState();
		} else if (type == "power") {
			panel.webSetPower(boolValue(property(payload, "on")));
			pushState();
		} else if (type == "preset") {
			panel.webSetPreset(stringValue(property(payload, "id")));
			pushState();
		} else if (type == "output") {
			panel.webSetOutput(stringValue(property(payload, "id")));
			pushOutputs(true);
		} else if (type == "effect") {
			panel.webSetEffect(
				(int)doubleValue(property(payload, "index")),
				doubleValue(property(payload, "value")));
		} else if (type == "eq-band") {
			panel.webSetEqBand(
				(int)doubleValue(property(payload, "index")),
				doubleValue(property(payload, "value")));
		} else if (type == "eq-reset") {
			panel.webSetEqValues(property(payload, "values"));
		} else if (type == "save-preset") {
			panel.webSavePreset(
				stringValue(property(payload, "name")));
		} else if (type == "import-presets") {
			panel.webImportPresets();
		} else if (type == "export-preset") {
			panel.webExportPreset();
		} else if (type == "theme") {
			panel.webSetThemeMode(
				stringValue(property(payload, "theme")));
		} else if (type == "accent") {
			panel.webSetAccent(
				stringValue(property(payload, "accent")));
		} else if (type == "autostart") {
			panel.webSetAutostart(
				boolValue(property(payload, "enabled")));
			pushState();
		} else if (type == "minimize") {
			if (onMinimize)
				onMinimize();
		} else if (type == "close") {
			if (onClose)
				onClose();
		} else if (type == "quit") {
			if (onQuit)
				onQuit();
			else if (auto *app = juce::JUCEApplication::getInstance())
				app->systemRequestedQuit();
		} else if (type == "drag") {
			beginWindowMove(*getTopLevelComponent());
		}
	}

	void pushState()
	{
		if (!ready || !isShowing() || browser == nullptr)
			return;
		browser->emitEventIfBrowserIsVisible("saucefxState",
						     panel.createWebState());
	}

	void pushSpectrum()
	{
		if (!ready || !isShowing() || browser == nullptr)
			return;
		auto bands = panel.webSpectrum();
		juce::Array<juce::var> values;
		for (auto band : bands)
			values.add((double)band);
		browser->emitEventIfBrowserIsVisible("saucefxSpectrum",
						     values);
	}

	void pushOutputs(bool force = false)
	{
		if (!ready || !isShowing() || browser == nullptr)
			return;
		auto state = panel.createWebOutputs();
		auto json = juce::JSON::toString(state, true);
		if (!force && json == lastOutputs_)
			return;
		lastOutputs_ = json;
		/* Device notifications must not reset the web UI's EQ/effect
		 * baselines or discard unsaved edits. */
		browser->emitEventIfBrowserIsVisible("saucefxOutputs", state);
	}

	void pushToast(const juce::String &message)
	{
		if (!ready || !isShowing() || browser == nullptr ||
		    message.isEmpty())
			return;
		browser->emitEventIfBrowserIsVisible("saucefxToast", message);
	}

	void timerCallback() override
	{
		pushSpectrum();
		if (++outputTicks_ % 15 == 0)
			pushOutputs();
	}

	SauceFXPanel &panel;
	juce::File root;
	std::unique_ptr<juce::WebBrowserComponent> browser;
	juce::Label errorLabel;
	bool ready = false;
	int outputTicks_ = 0;
	juce::String lastOutputs_;
};

/* ================================================================== */
/* Settings gear, next to the power button in the title bar            */

class FxSettingsButton : public FxPressButton
{
public:
	FxSettingsButton() : FxPressButton("settingsButton")
	{
		setMouseCursor(juce::MouseCursor::PointingHandCursor);
		setWantsKeyboardFocus(true);
	}

	void setOpen(bool open)
	{
		setToggleState(open, juce::dontSendNotification);
		repaint();
	}

	void paintButton(juce::Graphics &g, bool over, bool down) override
	{
		auto r = getLocalBounds().toFloat();
		float scale = pressScale(0.92f);
		g.addTransform(fxScaleAround(scale, r.getCentre()));
		if (over || down || getToggleState()) {
			g.setColour(fxc(DefaultText,
					down ? 0.16f
					     : (getToggleState() ? 0.10f : 0.08f)));
			g.fillEllipse(
				r.withSizeKeepingCentre(28.0f, 28.0f));
		}

		auto &t = FxTheme::get();
		auto &d = getToggleState() ? t.settingsGearActive
					   : t.settingsGear;
		drawWithinIf(g, d, r);
	}
};

/* A short cross-fade between dark and light: snapshot the old theme,
 * swap the palette, then dissolve the snapshot over the new UI. Only
 * opacity changes, so the controls underneath stay interactive. */
class FxThemeFade : public juce::Component, private juce::Timer
{
public:
	explicit FxThemeFade(juce::Image image)
		: image_(std::move(image))
	{
		setOpaque(false);
		setInterceptsMouseClicks(false, false);
	}

	void begin()
	{
		startMs_ = juce::Time::getMillisecondCounterHiRes();
		alpha_ = 1.0f;
		startTimerHz(60);
	}

	void paint(juce::Graphics &g) override
	{
		if (!image_.isValid())
			return;
		g.setOpacity(alpha_);
		g.drawImageAt(image_, 0, 0);
	}

private:
	void timerCallback() override
	{
		constexpr double durationMs = 190.0;
		const double elapsed = juce::Time::getMillisecondCounterHiRes() -
				       startMs_;
		const float t = (float)juce::jlimit(
			0.0, 1.0, elapsed / durationMs);
		alpha_ = 1.0f - fxEaseOutCubic(t);
		repaint();

		if (t >= 1.0f) {
			stopTimer();
			setVisible(false);
		}
	}

	juce::Image image_;
	double startMs_ = 0.0;
	float alpha_ = 1.0f;
};

/* ================================================================== */
/* The main window: SauceFX title bar (custom, FxTheme-styled) hosting
/* the power + settings buttons next to the window buttons             */

class SauceFXWindow : public juce::DocumentWindow
{
public:
	SauceFXWindow(SauceFXPanel *panel, bool powerOn,
		      bool addToDesktop = true, bool webMode = false)
		: juce::DocumentWindow("SauceFX", juce::Colours::black,
				       juce::DocumentWindow::closeButton,
				       addToDesktop)
		, panel_(panel), webMode_(webMode)
	{
		/* rounded corners like the original app (FxTheme
		 * WINDOW_CORNER_RADIUS 21): transparent ARGB window, no
		 * JUCE frame, fixed size (no resize / maximise /
		 * fullscreen) */
		setOpaque(false);
		setResizable(false, false);

		if (webMode_) {
			/* The React header is the title bar. Keep the native
			 * window chrome out of the way and put WebKit over the
			 * entire 1040x568 surface. */
			setTitleBarHeight(0);
			setTitleBarButtonsRequired(0, false);
			if (panel_ != nullptr) {
				web_ = std::make_unique<WebInterface>(*panel_);
				web_->onMinimize = [this] {
					setMinimised(true);
				};
				web_->onClose = [this] { closeButtonPressed(); };
				web_->onQuit = [] {
					if (auto *app =
						    juce::JUCEApplication::
							    getInstance())
						app->systemRequestedQuit();
				};
				setContentNonOwned(web_.get(), true);
			}
			return;
		}

		if (panel_ != nullptr) {
			panel_->onThemeChange =
				[this](int mode, bool customAccent,
				       uint32_t accent) {
					applyTheme(mode, customAccent, accent);
				};
			panel_->onSettingsVisibilityChanged = [this](bool open) {
				settingsButton.setOpen(open);
			};
		}

		powerButton.setSize(24, 24);
		powerButton.setPowerState(powerOn);
		powerButton.onClick = [this]() {
			const bool on = !powerButton.getPowerState();
			fprintf(stderr,
				"saucefx: power button clicked -> %d\n",
				on ? 1 : 0);
			powerButton.setPowerState(on);
			if (panel_)
				panel_->setPowerState(on);
		};
		addAndMakeVisible(powerButton);

		settingsButton.setSize(24, 24);
		settingsButton.onClick = [this]() {
			if (panel_ != nullptr)
				panel_->toggleSettings();
			settingsButton.setOpen(panel_ != nullptr &&
					       panel_->isSettingsVisible());
		};
		addAndMakeVisible(settingsButton);
	}

	~SauceFXWindow() override
	{
		/* web_ is a member and is destroyed before the DocumentWindow
		 * base; clear the non-owning content pointer first so the
		 * base destructor never touches a dangling component. */
		if (webMode_)
			clearContentComponent();
	}

	/* live theme switch: palette + drawables + LnF + explicit label
	 * colours, then persist the choice under the XDG state dir */
	void applyTheme(int mode, bool customAccent, uint32_t accent)
	{
		const int oldMode = fx_theme_mode_;
		juce::Image oldTheme;

		/* A mode switch is a discrete, occasional action, so a
		 * short cross-fade hides the large color jump. Accent
		 * dragging stays immediate: it is direct manipulation. */
		if (mode != oldMode && isVisible() && getWidth() > 0 &&
		    getHeight() > 0) {
			if (themeFade_ != nullptr)
				themeFade_.reset();
			oldTheme = createComponentSnapshot(
				getLocalBounds(), true, 1.0f);
		}

		FxTheme::setTheme(mode, customAccent, accent);
		if (auto *lf = dynamic_cast<FxLookAndFeel *>(&getLookAndFeel()))
			lf->applyTheme();
		if (panel_ != nullptr)
			panel_->refreshTheme();
		sendLookAndFeelChange();
		repaint();

		if (oldTheme.isValid()) {
			themeFade_ = std::make_unique<FxThemeFade>(
				std::move(oldTheme));
			addAndMakeVisible(*themeFade_);
			themeFade_->setBounds(getLocalBounds());
			themeFade_->toFront(false);
			themeFade_->begin();
		}

		saveGuiSettings(mode, customAccent, accent,
				fx_autostart_enabled_, fx_output_routed_);
	}

	/* no 1 px window frame (the original window has none) */
	juce::BorderSize<int> getBorderThickness() const override
	{
		return {};
	}

	void setPowerState(bool on) { powerButton.setPowerState(on); }

	void applyRoundedWindowShape()
	{
		if (webMode_)
			::applyRoundedWindowShape(*this);
	}

	void refreshWebState()
	{
		if (web_ != nullptr)
			web_->refreshState();
	}

	void visibilityChanged() override
	{
		if (webMode_ && isVisible())
			applyRoundedWindowShape();
	}

	void applyAppIcon()
	{
		auto icon = appIconImage();
		if (icon.isValid())
			if (auto *peer = getPeer())
				peer->setIcon(icon);
	}

	void closeButtonPressed() override
	{
		/* Closing is "put SauceFX in the background", not "stop the
		 * audio engine". The engine and virtual sink live in this
		 * process, so they stay alive while the window is hidden.
		 * Launching SauceFX again drops a show.request marker and
		 * this instance brings the window back. Use Settings ->
		 * Quit SauceFX to actually stop the engine. */
		if (panel_ != nullptr) {
			panel_->savePresetSessionNow();
			panel_->hideSettings();
		}
		setVisible(false);
		fprintf(stderr,
			"saucefx: window hidden; DSP continues (run SauceFX "
			"again to reopen, or use Settings -> Quit SauceFX)\n");
	}

	void resized() override
	{
		DocumentWindow::resized();

		if (webMode_) {
			if (web_ != nullptr)
				web_->setBounds(getLocalBounds());
			return;
		}

		/* GNOME-style close button: 34 px circle, 6 px margin;
		 * the power button sits 20 px to its left, and the
		 * settings gear another 20 px left of that */
		int barH = getTitleBarHeight();
		int powerX = getWidth() - 6 - 34 - 20 -
			     powerButton.getWidth();
		powerButton.setBounds(
			powerX, (barH - powerButton.getHeight()) / 2,
			powerButton.getWidth(), powerButton.getHeight());

		int settingsX = powerX - 20 - settingsButton.getWidth();
		settingsButton.setBounds(
			settingsX, (barH - settingsButton.getHeight()) / 2,
			settingsButton.getWidth(),
			settingsButton.getHeight());

		if (themeFade_ != nullptr)
			themeFade_->setBounds(getLocalBounds());
	}

private:
	FxPowerButton powerButton;
	FxSettingsButton settingsButton;
	std::unique_ptr<FxThemeFade> themeFade_;
	std::unique_ptr<WebInterface> web_;
	SauceFXPanel *panel_ = nullptr;
	bool webMode_ = false;
};

/* ================================================================== */

class SauceFXApp : public juce::JUCEApplication,
		   private juce::Timer
{
public:
	SauceFXApp() = default;

	const juce::String getApplicationName() override
	{
		return "SauceFX";
	}
	const juce::String getApplicationVersion() override
	{
		return SAUCEFX_VERSION;
	}
	bool moreThanOneInstanceAllowed() override
	{
		return true;
	}

	void initialise(const juce::String &commandLine) override
	{
		/* XDG autostart launches pass --autostart so login starts
		 * the engine hidden ("in the background") instead of
		 * popping the window over the desktop. */
		const bool autoStartLaunch =
			commandLine.contains("--autostart") ||
			commandLine.contains("--background");

		/* Character handling for UTF-8 preset names/paths. Keep
		 * LC_NUMERIC at "C" so the .fac numeric formatting and
		 * parsing always use a dot decimal separator. */
		std::setlocale(LC_CTYPE, "");

		/* WebKit's child re-exec needs an absolute executable path;
		 * keep JUCE's currentExecutableFile resolvable when the
		 * process was started by a bare command name. */
		ensureResolvableExecutablePath();

		/* Single-instance guard: flock the app lockfile for the
		 * whole process lifetime. A second launch asks this
		 * instance to show its window instead of spawning a
		 * second engine/sink. */
		{
			auto dir = stateDir();
			dir.createDirectory();
			auto lockFile = dir.getChildFile("app.lock");
			FILE *lf = fopen(
				lockFile.getFullPathName().toRawUTF8(), "w");
			if (lf != NULL) {
				if (flock(fileno(lf), LOCK_EX | LOCK_NB) != 0) {
					fclose(lf);
					if (!autoStartLaunch) {
						/* Ask the existing instance
						 * to show its window (it may
						 * be hidden in the
						 * background), then leave
						 * without starting a second
						 * engine. */
						auto request =
							showRequestFile();
						if (!request.replaceWithText(
							    "show\n", false,
							    false, "\n"))
							fprintf(stderr,
								"saucefx: could not write %s\n",
								request.getFullPathName().toRawUTF8());
						fprintf(stderr,
							"saucefx: another SauceFX "
							"instance is running - asked "
							"it to show its window\n");
					} else {
						/* Login autostart should never
						 * pop the window over an
						 * already-running instance. */
						fprintf(stderr,
							"saucefx: autostart launch ignored; "
							"another instance is running\n");
					}
					quit();
					return;
				}
				/* keep the FILE* (and lock) for the
				 * process lifetime */
				instance_lock_ = lf;
			}
		}

		/* restore the saved dark/light, accent and startup choices
		 * before the look-and-feel and first window are built */
		auto guiSettings = loadGuiSettings();
		fx_autostart_enabled_ = guiSettings.autostart;
		fx_output_routed_ = guiSettings.outputRouted;
		fx_output_device_ = guiSettings.outputDevice;
		fx_output_port_ = guiSettings.outputPort;

		/* keep the XDG entry in sync with the setting even if it
		 * was deleted, disabled from outside, or the binary moved */
		if (!applyAutostartSetting(guiSettings.autostart))
			fprintf(stderr,
				"saucefx: autostart entry could not be updated\n");

		if (autoStartLaunch && !guiSettings.autostart) {
			fprintf(stderr,
				"saucefx: autostart is disabled; leaving\n");
			quit();
			return;
		}

		FxTheme::setTheme(guiSettings.themeMode,
				  guiSettings.customAccent,
				  guiSettings.accent);

		char err[192] = "";
		look = std::make_unique<FxLookAndFeel>();
		juce::LookAndFeel::setDefaultLookAndFeel(look.get());

		engine = fxpw_start(
			"SauceFX", nullptr, fx_output_device_.toRawUTF8(),
			/* make_default */ 0, /* bypass */ 0,
			/* stats */ 0, /* quiet */ 0, err, sizeof(err));

		if (engine == NULL) {
			juce::AlertWindow::showMessageBoxAsync(
				juce::MessageBoxIconType::WarningIcon,
				"SauceFX",
				juce::String("cannot start engine: ") +
					juce::String(err));
			quit();
			return;
		}

		/* Restore the last output choice before building the panel,
		 * so the combo reflects the real default sink. With
		 * autostart this is what makes a hidden launch actually
		 * process system audio after login. */
		fxpw_set_output(engine, fx_output_device_.toRawUTF8(), fx_output_port_.toRawUTF8());
		if (guiSettings.outputRouted &&
		    fxpw_route_default(engine, 1) != 0)
			fprintf(stderr,
				"saucefx: could not restore SauceFX output routing\n");
		fx_output_routed_ = fxpw_is_routed_default(engine) != 0;

		saveGuiSettings(guiSettings.themeMode,
				guiSettings.customAccent, guiSettings.accent,
				fx_autostart_enabled_, fx_output_routed_);

		panel = std::make_unique<SauceFXPanel>(engine);

		window = std::make_unique<SauceFXWindow>(
			panel.get(), fxpw_get_power(engine) != 0,
			!autoStartLaunch, /* webMode */ true);
		window->setUsingNativeTitleBar(false);
		/* The WebInterface fills the whole 1040x568 window; the React
		 * header doubles as the title bar. */
		window->centreWithSize(1040, 568);
		window->applyAppIcon();
		if (autoStartLaunch) {
			/* /etc/xdg/autostart is expected to be quiet. The
			 * window was built without a desktop peer, so no
			 * native window appears at login; the engine and
			 * sink are live in the background. Running SauceFX
			 * again attaches the window and shows it. */
			window->setVisible(false);
			fprintf(stderr,
				"saucefx: autostart launch; running hidden in "
				"the background\n");
		} else {
			window->setVisible(true);
			window->toFront(true);
			window->applyRoundedWindowShape();
		}

		/* A stale show marker belongs to a previous session. Start
		 * polling for a second launch asking this hidden window to
		 * come back. */
		showRequestFile().deleteFile();
		startTimer(200);
	}

	void shutdown() override
	{
		stopTimer();
		if (panel != nullptr)
			panel->savePresetSessionNow();
		window.reset();
		panel.reset();
		if (engine != NULL) {
			fxpw_stop(engine);
			engine = nullptr;
		}
		juce::LookAndFeel::setDefaultLookAndFeel(nullptr);
		look.reset();
		if (instance_lock_ != NULL) {
			fclose(instance_lock_);
			instance_lock_ = NULL;
		}
	}

	void systemRequestedQuit() override
	{
		quit();
	}

private:
	void timerCallback() override
	{
		if (window == nullptr) {
			stopTimer();
			return;
		}

		auto request = showRequestFile();
		if (!request.existsAsFile())
			return;

		request.deleteFile();
		if (!window->isOnDesktop())
			window->addToDesktop();
		window->applyAppIcon();
		window->setMinimised(false);
		window->setVisible(true);
		window->toFront(true);
		window->applyRoundedWindowShape();
		window->refreshWebState();
		fprintf(stderr, "saucefx: window restored from background\n");
	}

	FXPW *engine = nullptr;
	std::unique_ptr<SauceFXPanel> panel;
	std::unique_ptr<SauceFXWindow> window;
	std::unique_ptr<FxLookAndFeel> look;
	FILE *instance_lock_ = NULL; /* held for the process lifetime */
};

START_JUCE_APPLICATION(SauceFXApp)
