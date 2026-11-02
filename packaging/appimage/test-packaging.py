#!/usr/bin/env python3
"""Small loader tests; no JUCE build, graphics session or audio server needed."""
import os
import shlex
import subprocess
import tempfile
from pathlib import Path

directory = Path(__file__).resolve().parent
cflags = shlex.split(subprocess.check_output(["pkg-config", "--cflags", "gio-2.0"], text=True))
with tempfile.TemporaryDirectory(prefix="saucefx-appimage-") as temp:
    appdir = Path(temp) / "Portable App"
    libdir = appdir / "usr/lib"
    (appdir / "usr/bin").mkdir(parents=True)
    libdir.mkdir()
    subprocess.run(["cc", "-shared", "-fPIC", "-O2", *cflags,
        str(directory / "webkit-paths.c"), "-ldl", "-o", str(libdir / "libsaucefx-webkit-paths.so")], check=True)
    helperdir = libdir / "x86_64-linux-gnu/webkit2gtk-4.1"
    helperdir.mkdir(parents=True)
    helper = helperdir / "WebKitNetworkProcess"
    helper.write_text('#!/bin/sh\nprintf "%s" "$1"\n')
    helper.chmod(0o755)
    injected = helperdir / "injected-bundle"
    injected.mkdir()
    stub = Path(temp) / "stub.c"
    stub.write_text('int bundled_value(void) { return 42; }\n')
    subprocess.run(["cc", "-shared", "-fPIC", str(stub), "-o",
        str(injected / "libwebkit2gtkinjectedbundle.so")], check=True)
    program = Path(temp) / "check.c"
    program.write_text(r'''
#include <gio/gio.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
int main(int argc, char **argv) {
    if (argc != 2) return 1;
    /* Match JUCE's RTLD_LOCAL GTK/GIO scope: the shim must be able to find
     * the original GIO function without promoting those libraries globally. */
    void *gio = dlopen("libgio-2.0.so.0", RTLD_NOW | RTLD_LOCAL);
    if (!gio) return 8;
    GSubprocessLauncher *(*new_launcher)(GSubprocessFlags) = dlsym(gio, "g_subprocess_launcher_new");
    GSubprocess *(*spawn)(GSubprocessLauncher *, const gchar * const *, GError **) =
        dlsym(RTLD_DEFAULT, "g_subprocess_launcher_spawnv");
    gboolean (*communicate)(GSubprocess *, const char *, GCancellable *, char **, char **, GError **) =
        dlsym(gio, "g_subprocess_communicate_utf8");
    if (!new_launcher || !spawn || !communicate) return 9;
    GError *error = NULL;
    GSubprocessLauncher *launcher = new_launcher(G_SUBPROCESS_FLAGS_STDOUT_PIPE);
    const char *args[] = { "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitNetworkProcess", argv[1], NULL };
    GSubprocess *process = spawn(launcher, args, &error);
    if (!process) { fprintf(stderr, "%s\n", error->message); return 2; }
    char *text = NULL;
    if (!communicate(process, NULL, NULL, &text, NULL, &error)) return 3;
    if (strcmp(text, argv[1])) return 4;
    void *bundle = dlopen("/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/injected-bundle/libwebkit2gtkinjectedbundle.so", RTLD_LAZY);
    if (!bundle) { fprintf(stderr, "%s\n", dlerror()); return 5; }
    int (*value)(void) = dlsym(bundle, "bundled_value");
    if (!value || value() != 42) return 6;
    char cwd[4096];
    if (!getcwd(cwd, sizeof(cwd))) return 7;
    printf("%s\n%s\n%s\n%s\n", getenv("APPDIR"), getenv("SAUCEFX_ROOT"), cwd, text);
    return 0;
}
''')
    executable = appdir / "usr/bin/SauceFX"
    subprocess.run(["cc", *cflags, str(program), "-ldl", "-o", str(executable)], check=True)
    environment = dict(os.environ, APPDIR=str(appdir))
    environment.pop("XDG_DATA_DIRS", None)
    result = subprocess.check_output(
        ["bash", str(directory / "AppRun"), "argument with spaces"],
        env=environment, cwd=temp, text=True,
    ).splitlines()
    assert result == [str(appdir), str(appdir / "usr/share/saucefx"), str(appdir / "usr"), "argument with spaces"]
print("PASS: AppRun preserves paths with spaces and relocates WebKit helpers and its injected bundle")
