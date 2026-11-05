# SauceFX - Linux sound EQ and audio enhancement app: DfxDsp static library + WAV harness
#
# Builds the 94-file DSP source list in src/sources.txt on Fedora with GCC,
# plus the shared helper modules (audiopassthru/src/pstr,MTH,FILE,SLOUT)
# that the DSP links against, plus SauceFX's POSIX shims (src/shims/*.cpp,
# replacing the Win32-only reg layer).
# Run from anywhere: make -C src
# Run the harness:   make -C src run IN=audio.wav [PRESET="Jazz"]

ROOT       := ..
DSP        := $(ROOT)/dsp
SRC_DIR    := .
BUILD      := build
OBJDIR     := $(BUILD)/obj
BIN        := $(BUILD)/bin
LIB        := $(BUILD)/libdfxdsp_linux.a
HARNESS    := $(BIN)/fxdspharness
EQ_REGISTRY_TEST := $(BIN)/eqregistrytest
EQ_RACE_ITERS ?= 250

CC  := gcc
CXX := g++

# Same preprocessor state as the original DfxDsp build (software-DSP target),
# plus Linux. WIN32 is deliberately NOT defined (see docs/ENGINEERING.md).
DEFINES := -DDSPSOFT_TARGET -DPT_DSP_BUILD=PT_DSP_DFX -DPT_NON_MFC -DNDEBUG

# -funsigned-char matches MSVC default char signedness.
# -fno-strict-aliasing: legacy C DSP code predates strict-aliasing rules.
COMMON   := -O2 -funsigned-char -fno-strict-aliasing -w
CFLAGS   := $(COMMON) -std=c99 -MMD
CXXFLAGS := $(COMMON) -std=c++17 -MMD

# Include order mirrors the MSVC project + the SauceFX shim dir first so the
# stubs (windows.h, mmsystem.h, ...) win.
INCLUDES := -I$(SRC_DIR)/include \
            -I$(DSP)/include \
            -I$(DSP)/ptutil/include \
            -I$(DSP)/ptutil/dfxp \
            -I$(DSP)/ptutil \
            -I$(DSP)/ptutil/PWAV \
            -I$(DSP)/../audiopassthru/include

# Source list: repo-root-relative paths in src/sources.txt.
SRCS := $(shell cat $(SRC_DIR)/sources.txt)
OBJS := $(patsubst %,$(OBJDIR)/%,$(patsubst %.cpp,%.o,$(patsubst %.c,%.o,$(SRCS))))
SRCDEPS := $(addprefix $(ROOT)/,$(SRCS))

.PHONY: all run test test-eq-registry test-audio-latency test-output-routing rt clean

all: $(HARNESS)

$(OBJDIR)/%.o: $(ROOT)/%.c $(SRC_DIR)/sources.txt | $(OBJDIR)
	@mkdir -p $(dir $@)
	$(CC) $(CFLAGS) $(DEFINES) $(INCLUDES) -c -o $@ $<

$(OBJDIR)/%.o: $(ROOT)/%.cpp $(SRC_DIR)/sources.txt | $(OBJDIR)
	@mkdir -p $(dir $@)
	$(CXX) $(CXXFLAGS) $(DEFINES) $(INCLUDES) -c -o $@ $<

$(LIB): $(OBJS) $(SRCDEPS) $(SRC_DIR)/sources.txt
	@mkdir -p $(dir $@)
	ar rcs $(LIB) $(OBJS)

$(HARNESS): $(SRC_DIR)/harness/fxdspharness.cpp $(LIB) | $(BIN)
	@mkdir -p $(dir $@)
	$(CXX) $(CXXFLAGS) -I$(DSP)/include -I$(DSP) -I$(SRC_DIR)/include \
		-o $@ $(SRC_DIR)/harness/fxdspharness.cpp $(LIB) -lm

run: $(HARNESS)
	$(HARNESS) $(IN) $(OUT) $(PRESET)

$(EQ_REGISTRY_TEST): $(SRC_DIR)/harness/eqregistrytest.cpp $(LIB) | $(BIN)
	$(CXX) $(CXXFLAGS) -I$(DSP)/include -I$(DSP) -I$(SRC_DIR)/include \
		-o $@ $< $(LIB) -pthread -lm

test-eq-registry: $(EQ_REGISTRY_TEST)
	$(EQ_REGISTRY_TEST) $(EQ_RACE_ITERS)

# Sink lifecycle regressions: restart reaping (SIGKILL + relaunch) and
# live-sibling preservation. Needs pactl, plus unprivileged user/PID
# namespaces for the restart test; scripts exit 77 when unavailable.
test: test-eq-registry test-audio-latency rt test-output-routing
	@bash $(SRC_DIR)/harness/sinkrestarttest.sh "$(abspath $(RT))"; \
		rc=$$?; [ $$rc -eq 0 ] || [ $$rc -eq 77 ]
	@bash $(SRC_DIR)/harness/sinklivesiblingtest.sh "$(abspath $(RT))"; \
		rc=$$?; [ $$rc -eq 0 ] || [ $$rc -eq 77 ]

# PipeWire bridge (library form + CLI). Needs
# pipewire-devel and pulseaudio-libs-devel installed.
PW_CFLAGS := $(shell pkg-config --cflags libpipewire-0.3 libpulse 2>/dev/null)
PW_LIBS   := $(shell pkg-config --libs libpipewire-0.3 libpulse 2>/dev/null)
RT        := $(BIN)/saucefxrt
PWDIR     := $(SRC_DIR)/pwbridge
PWBRIDGE_LIB := $(BUILD)/libfxpw.a
OUTPUT_ROUTING_TEST := $(BIN)/outputroutingtest
AUDIO_LATENCY_TEST := $(BIN)/audiolatencytest
PW_DFLAGS := -I$(PWDIR) -I$(DSP)/include -I$(DSP)/ptutil/include -I$(DSP) -I$(SRC_DIR)/include

rt: $(RT)

$(BUILD)/libfxpw.a: $(BUILD)/pwbridge/fxpw.o $(BUILD)/pwbridge/outputs.o
	ar rcs $@ $^

$(BUILD)/pwbridge/%.o: $(PWDIR)/%.cpp $(PWDIR)/fxpw.h $(PWDIR)/outputs.h $(LIB)
	@mkdir -p $(dir $@)
	$(CXX) $(CXXFLAGS) $(DEFINES) $(PW_DFLAGS) $(PW_CFLAGS) \
		-c -o $@ $< $(PW_LIBS)

$(RT): $(PWDIR)/saucefxrt.cpp $(PWBRIDGE_LIB) | $(BIN)
	@mkdir -p $(dir $@)
	$(CXX) $(CXXFLAGS) $(PW_DFLAGS) $(PW_CFLAGS) -o $@ $< \
		$(PWBRIDGE_LIB) $(LIB) $(PW_LIBS) -lpthread -lm

$(OUTPUT_ROUTING_TEST): $(SRC_DIR)/harness/outputroutingtest.cpp $(PWDIR)/outputs.h $(PWBRIDGE_LIB) | $(BIN)
	$(CXX) $(CXXFLAGS) $(PW_DFLAGS) $(PW_CFLAGS) -o $@ $< \
		$(PWBRIDGE_LIB) $(LIB) $(PW_LIBS) -lpthread -lm

test-output-routing: $(OUTPUT_ROUTING_TEST)
	@bash $(SRC_DIR)/harness/outputroutingtest.sh "$(abspath $(OUTPUT_ROUTING_TEST))"; \
		rc=$$?; [ $$rc -eq 0 ] || [ $$rc -eq 77 ]

$(AUDIO_LATENCY_TEST): $(SRC_DIR)/harness/audiolatencytest.cpp $(PWDIR)/fxpw.cpp $(PWDIR)/fxpw.h $(PWBRIDGE_LIB) | $(BIN)
	$(CXX) $(CXXFLAGS) $(DEFINES) $(PW_DFLAGS) $(PW_CFLAGS) -o $@ $< \
		$(PWBRIDGE_LIB) $(LIB) $(PW_LIBS) -lpthread -lm

test-audio-latency: $(AUDIO_LATENCY_TEST)
	$(AUDIO_LATENCY_TEST)

-include $(wildcard $(OBJDIR)/*/*.d) $(wildcard $(OBJDIR)/*.d)

clean:
	rm -rf $(BUILD)

$(OBJDIR) $(BIN):
	mkdir -p $@
