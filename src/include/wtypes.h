/*
 * SauceFX Linux app. MSVC <wtypes.h> stub for reg helpers.
 */
#pragma once
#include <windows.h>

typedef unsigned long REGSAM;

/* HKEY placeholders; the Linux reg fallback ignores them. */
typedef int REG_KEY_TYPE;
#define REG_CURRENT_USER 0
#define REG_LOCAL_MACHINE 1
#define REG_CLASSES_ROOT 2
