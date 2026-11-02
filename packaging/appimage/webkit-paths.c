/* Relocate only WebKit's private helper processes and injected bundle.
 * Keep system library paths and WebKit's sandbox mount rules unchanged. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <gio/gio.h>
#include <stdlib.h>
#include <string.h>

static char *portable_path(const char *path)
{
    if (path == NULL)
        return NULL;
    const char *root = getenv("APPDIR");
    if (root == NULL || root[0] != '/')
        return NULL;
    const char *prefixes[] = {
        "/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/",
        "/usr/libexec/webkit2gtk-4.1/"
    };
    for (size_t i = 0; i < sizeof(prefixes) / sizeof(prefixes[0]); ++i) {
        if (strncmp(path, prefixes[i], strlen(prefixes[i])) == 0) {
            size_t size = strlen(root) + strlen(path) + 1;
            char *result = malloc(size);
            if (result != NULL) {
                strcpy(result, root);
                strcat(result, path);
            }
            return result;
        }
    }
    return NULL;
}

void *dlopen(const char *filename, int flags)
{
    void *(*original)(const char *, int) = dlsym(RTLD_NEXT, "dlopen");
    if (original == NULL) {
        errno = ENOSYS;
        return NULL;
    }
    char *mapped = portable_path(filename);
    void *result = original(mapped != NULL ? mapped : filename, flags);
    free(mapped);
    return result;
}

GSubprocess *g_subprocess_launcher_spawnv(GSubprocessLauncher *launcher,
    const gchar * const *argv, GError **error)
{
    typedef GSubprocess *(*Spawn)(GSubprocessLauncher *, const gchar * const *, GError **);
    Spawn original = dlsym(RTLD_NEXT, "g_subprocess_launcher_spawnv");
    void *gio = NULL;
    /* JUCE can load GTK/GIO in a local dlopen scope. */
    if (original == NULL) {
        void *(*open_original)(const char *, int) = dlsym(RTLD_NEXT, "dlopen");
        if (open_original != NULL)
            gio = open_original("libgio-2.0.so.0", RTLD_LAZY | RTLD_NOLOAD);
        if (gio != NULL)
            original = dlsym(gio, "g_subprocess_launcher_spawnv");
    }
    if (original == NULL) {
        if (gio != NULL) dlclose(gio);
        errno = ENOSYS;
        return NULL;
    }
    size_t count = 0;
    while (argv[count] != NULL) ++count;
    const char *preload = getenv("LD_PRELOAD");
    int bwrap = count > 0 && strcmp(argv[0], "/usr/bin/bwrap") == 0 && preload != NULL;
    const char **arguments = calloc(count + 4, sizeof(char *));
    char **mapped = calloc(count, sizeof(char *));
    if (arguments == NULL || mapped == NULL) {
        free(arguments); free(mapped);
        if (gio != NULL) dlclose(gio);
        errno = ENOMEM;
        return NULL;
    }
    size_t next = 0;
    for (size_t i = 0; i < count; ++i) {
        /* Some distros use setuid bwrap, which removes LD_PRELOAD. Pass
         * the bundled shim explicitly into the already-confined child.
         * LD_LIBRARY_PATH's AppDir mounts are provided by WebKit itself. */
        if (bwrap && strcmp(argv[i], "--") == 0) {
            arguments[next++] = "--setenv";
            arguments[next++] = "LD_PRELOAD";
            arguments[next++] = preload;
            bwrap = 0;
        }
        mapped[i] = portable_path(argv[i]);
        arguments[next++] = mapped[i] != NULL ? mapped[i] : argv[i];
    }
    GSubprocess *result = original(launcher, arguments, error);
    for (size_t i = 0; i < count; ++i) free(mapped[i]);
    free(mapped); free(arguments);
    if (gio != NULL) dlclose(gio);
    return result;
}
