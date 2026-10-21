/*
 * SauceFX Linux app: Win32 compatibility shim.
 *
 * Stub for dsp/u_DfxDsp.h. The DSP core includes AudioPassthru.h but never
 * calls into the device layer; the real header belongs to the Windows
 * virtual-audio-driver interface (audiopassthru/), which is replaced by
 * PipeWire/JUCE in the desktop app. Verified (2026-10): no dsp/*.cpp references any
 * passthru symbol, so an empty stub is lossless for the DSP core.
 */
#pragma once
