/*
 * SauceFX Linux app. MSVC <winbase.h> stub — includes in
 * audiopassthru/src/pstr transitively required winbase declarations that
 * survey showed resolve to kernel32 handles; nothing is called at link
 * time from the DSP path beyond generalized heap/light file handling.
 */
#pragma once
#include <windows.h>
