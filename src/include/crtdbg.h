/*
 * SauceFX Linux app: Win32 compatibility shim.
 *
 * Minimal MSVC <crtdbg.h> replacement. The DSP sources only use the
 * assert macros; debug-crash reporting machinery is deliberately absent.
 */
#pragma once
#include <assert.h>

#ifndef _ASSERT_EXPR
#define _ASSERT_EXPR(expr, msg) ((void)0)
#endif
#ifndef _ASSERT
#define _ASSERT(expr) ((void)0)
#endif
#ifndef _ASSERTE
#define _ASSERTE(expr) assert(expr)
#endif
#ifndef _CrtDbgBreak
#define _CrtDbgBreak() ((void)0)
#endif
#ifndef _CrtDbgReport
#define _CrtDbgReport(...) (0)
#endif
#ifndef _RPTF
#define _RPTF(...)
#endif
#ifndef _RPT
#define _RPT(...)
#endif
