# SauceFX - Linux sound EQ and audio enhancement app: JUCE desktop front-end.
#
# Prerequisites (built once by the DSP Makefile, linked here as-is):
#   make -C src           # produces ../build/libdfxdsp_linux.a
#   sudo dnf install cmake gcc-c++ nodejs npm \
#     webkit2gtk4.1-devel gtk3-devel libsoup3-devel \
#     libX11-devel libXext-devel libXinerama-devel libXrandr-devel \
#     libXcursor-devel freetype-devel fontconfig-devel
#
# Node.js must satisfy Vite 8: ^20.19.0 or >=22.12.0.
#
# JUCE 9.x is not vendored. Pass its path explicitly when it is not at the
# default sibling location:
#   cmake -S src/gui -B src/gui/build -DJUCE_DIR=/path/to/JUCE
#
# The React interface in frontend/ is built automatically with npm and copied
# next to the executable as "web/".
#
# Build:  cmake -S src/gui -B src/gui/build   &&  cmake --build src/gui/build -j
# Run:    src/gui/build/saucefxgui_artefacts/SauceFX

cmake_minimum_required(VERSION 3.22)

project(SauceFX VERSION 0.2.4 LANGUAGES C CXX)

include(GNUInstallDirs)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Packaging builds can supply an already-built frontend/dist so the native
# build does not need Node/npm inside a Flatpak or RPM build root.
set(SAUCEFX_WEB_DIST_DIR "" CACHE PATH
    "Prebuilt frontend/dist directory; skips the npm build when set")

set(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
set(APP_ROOT  "${REPO_ROOT}/src")

set(JUCE_DIR "${REPO_ROOT}/../deps/JUCE" CACHE PATH
    "Path to a JUCE 9 checkout")
if(NOT EXISTS "${JUCE_DIR}/CMakeLists.txt")
    message(FATAL_ERROR
        "JUCE not found at ${JUCE_DIR}. Download JUCE 9 and configure with "
        "-DJUCE_DIR=/path/to/JUCE")
endif()

set(DSP_LIB   "${APP_ROOT}/build/libdfxdsp_linux.a")
if(NOT EXISTS ${DSP_LIB})
    message(FATAL_ERROR
        "Missing ${DSP_LIB}. Build the DSP library first: make -C ${APP_ROOT}")
endif()

find_package(PkgConfig REQUIRED)
pkg_check_modules(PIPEWIRE REQUIRED IMPORTED_TARGET libpipewire-0.3)
pkg_check_modules(PULSE REQUIRED IMPORTED_TARGET libpulse)
find_package(X11 REQUIRED)

set(WEB_DIR "${REPO_ROOT}/frontend")
set(WEB_STAMP "${CMAKE_CURRENT_BINARY_DIR}/saucefx-web.stamp")
set(WEB_OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/saucefxgui_artefacts/web")

if(SAUCEFX_WEB_DIST_DIR)
    get_filename_component(SAUCEFX_WEB_DIST_DIR
        "${SAUCEFX_WEB_DIST_DIR}" ABSOLUTE)
    if(NOT EXISTS "${SAUCEFX_WEB_DIST_DIR}/index.html")
        message(FATAL_ERROR
            "SAUCEFX_WEB_DIST_DIR does not contain index.html: "
            "${SAUCEFX_WEB_DIST_DIR}")
    endif()

    add_custom_command(
        OUTPUT "${WEB_STAMP}"
        COMMAND "${CMAKE_COMMAND}" -E rm -rf "${WEB_OUTPUT_DIR}"
        COMMAND "${CMAKE_COMMAND}" -E copy_directory
            "${SAUCEFX_WEB_DIST_DIR}" "${WEB_OUTPUT_DIR}"
        COMMAND "${CMAKE_COMMAND}" -E touch "${WEB_STAMP}"
        DEPENDS "${SAUCEFX_WEB_DIST_DIR}/index.html"
        COMMENT "Copying the prebuilt SauceFX web interface"
        VERBATIM
    )
else()
    find_program(NPM_EXECUTABLE npm REQUIRED)

    file(GLOB_RECURSE WEB_SOURCES CONFIGURE_DEPENDS
        "${WEB_DIR}/src/*"
        "${WEB_DIR}/public/*"
    )
    list(APPEND WEB_SOURCES
        "${WEB_DIR}/index.html"
        "${WEB_DIR}/vite.config.ts"
        "${WEB_DIR}/tsconfig.json"
        "${WEB_DIR}/tsconfig.app.json"
        "${WEB_DIR}/tsconfig.node.json"
        "${WEB_DIR}/package.json"
        "${WEB_DIR}/package-lock.json"
    )

    add_custom_command(
        OUTPUT "${WEB_STAMP}"
        COMMAND "${NPM_EXECUTABLE}" install --no-audit --no-fund
        COMMAND "${NPM_EXECUTABLE}" run build
        COMMAND "${CMAKE_COMMAND}" -E rm -rf "${WEB_OUTPUT_DIR}"
        COMMAND "${CMAKE_COMMAND}" -E copy_directory
            "${WEB_DIR}/dist" "${WEB_OUTPUT_DIR}"
        COMMAND "${CMAKE_COMMAND}" -E touch "${WEB_STAMP}"
        DEPENDS ${WEB_SOURCES}
        WORKING_DIRECTORY "${WEB_DIR}"
        COMMENT "Building the SauceFX web interface"
        VERBATIM
    )
endif()
add_custom_target(saucefx_web_ui DEPENDS "${WEB_STAMP}")

add_subdirectory(${JUCE_DIR} JUCE-build)

juce_add_gui_app(saucefxgui
    PRODUCT_NAME "SauceFX"
    COMPANY_NAME "saucefx"
    BUNDLE_ID io.github.sauucee.SauceFX
    NEEDS_WEB_BROWSER TRUE
    # needs the desktop's clipboard/wm flags; defaults are fine
)
add_dependencies(saucefxgui saucefx_web_ui)

target_sources(saucefxgui PRIVATE
    Main.cpp
    ../pwbridge/fxpw.cpp
    ../pwbridge/outputs.cpp
)

# Same preprocess state the DSP library was compiled with (its parameter
# structs and engine dispatch depend on these being identical).
target_compile_definitions(saucefxgui PRIVATE
    DSPSOFT_TARGET
    PT_DSP_BUILD=PT_DSP_DFX
    PT_NON_MFC
    NDEBUG
    SAUCEFX_VERSION="${PROJECT_VERSION}"
    SAUCEFX_INSTALL_DATADIR="${CMAKE_INSTALL_FULL_DATADIR}/saucefx"
)

target_include_directories(saucefxgui PRIVATE
    ${REPO_ROOT}/dsp/include
    ${REPO_ROOT}/dsp/ptutil/include
    ${REPO_ROOT}/dsp
    ${APP_ROOT}/include
    ${APP_ROOT}/pwbridge
)

target_link_libraries(saucefxgui PRIVATE
    juce::juce_gui_basics
    juce::juce_gui_extra
    X11::X11
    X11::Xext
    ${DSP_LIB}
    PkgConfig::PIPEWIRE
    PkgConfig::PULSE
    pthread
)

# GUI apps on desktops we build need X11 via JUCE's bundled helpers; JUCE
# handles its platform deps internally from here on.

# ---------------------------------------------------------------------------
# Linux installation layout (used by the RPM spec and the Flatpak manifest)
#
#   <prefix>/bin/SauceFX
#   <prefix>/share/saucefx/{web,assets,presets}
#   <prefix>/share/{applications,metainfo,icons}/...
#
# The native app resolves the installed data directory at compile time and
# falls back to the source tree when it is run from a checkout.
#
# Everything SauceFX owns is in the "Runtime" component. Package builders use
# `cmake --install ... --component Runtime` so JUCE's optional headers,
# CMake package files and helper binaries are not copied into the package.
# ---------------------------------------------------------------------------
install(PROGRAMS "$<TARGET_FILE:saucefxgui>"
    DESTINATION "${CMAKE_INSTALL_BINDIR}"
    COMPONENT Runtime)

install(DIRECTORY "${WEB_OUTPUT_DIR}/"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/saucefx/web"
    COMPONENT Runtime)
install(DIRECTORY "${REPO_ROOT}/assets/"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/saucefx/assets"
    COMPONENT Runtime
    PATTERN "Screenshots" EXCLUDE)
install(DIRECTORY "${REPO_ROOT}/presets/"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/saucefx/presets"
    COMPONENT Runtime)

install(FILES
    "${REPO_ROOT}/packaging/linux/io.github.sauucee.SauceFX.desktop"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/applications"
    COMPONENT Runtime)
install(FILES
    "${REPO_ROOT}/packaging/linux/io.github.sauucee.SauceFX.metainfo.xml"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/metainfo"
    COMPONENT Runtime)
install(FILES
    "${REPO_ROOT}/assets/Images/saucefx-logo.svg"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/scalable/apps"
    RENAME "io.github.sauucee.SauceFX.svg"
    COMPONENT Runtime)
install(FILES
    "${REPO_ROOT}/assets/Images/saucefx-logo-128.png"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/128x128/apps"
    RENAME "io.github.sauucee.SauceFX.png"
    COMPONENT Runtime)
install(FILES
    "${REPO_ROOT}/assets/Images/saucefx-logo-512.png"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/icons/hicolor/512x512/apps"
    RENAME "io.github.sauucee.SauceFX.png"
    COMPONENT Runtime)

install(FILES "${REPO_ROOT}/LICENSE"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/licenses/saucefx"
    COMPONENT Runtime)
install(FILES "${REPO_ROOT}/README.md"
    DESTINATION "${CMAKE_INSTALL_DATADIR}/doc/saucefx"
    COMPONENT Runtime)
