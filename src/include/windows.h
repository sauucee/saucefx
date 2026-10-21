/*
 * SauceFX Linux app: Win32 compatibility shim.
 *
 * Minimal <windows.h> replacement covering only the Win32 surface the
 * DSP core and helper modules reference when built for Linux
 * (surveyed 2026-10). Everything Windows-exclusive (registry layout,
 * devices, threads) is behind memory/file abstractions and POSIX shims
 * (src/shims/misc_linux.cpp, reg_linux.cpp) rather than here.
 *
 * In-particular deliberate choices:
 *  - WIN32 is NOT defined, so Windows-only branches stay dormant.
 *  - swprintf keeps the legacy MSVC 2-arg call form everywhere (the DSP
 *    only ever calls that form), funneled into one implementation with a
 *    1024-wchar bound (>= every destination size in dsp/, which is
 *    PT_MAX_PATH_STRLEN or smaller).
 *  - Filetime conversions and GUID generation are self-contained.
 */
#pragma once

#ifndef _PORT_WINDOWS_SHIM_H
#define _PORT_WINDOWS_SHIM_H

#include <unistd.h>

#ifdef __cplusplus
#include <cwchar>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cerrno>
#include <ctime>
#else
#include <wchar.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <errno.h>
#include <time.h>
#endif

/* POSIX stat wrappers in MSVC naming (FileGeneral.cpp). The wide ones convert
 * the path via wcstombs; fields used: st_mode, st_size, st_mtime. */
#ifndef _PORT_STAT_SHIM
#define _PORT_STAT_SHIM
#ifdef __cplusplus

/* glibc's bits/stat.h defines st_mtime/st_atime/st_ctime as macros
 * (st_mtim.tv_sec accessors) for POSIX struct stat; they would garble
 * our own struct members below, so clear them first. glibc struct stat's
 * real members are untouched for code further down the TU. */
#undef st_mtime
#undef st_atime
#undef st_ctime
#undef st_birthtime

struct _stat64i32 {
	unsigned long st_mode;
	long long st_size;
	time_t st_mtime;
	time_t st_atime;
	time_t st_ctime;
};
struct _stati64 {
	unsigned long st_mode;
	long long st_size;
	time_t st_mtime;
	time_t st_atime;
	time_t st_ctime;
};

extern "C" int _sfxFillStatWide(const wchar_t *wcp_path, unsigned long *mode,
                                     long long *size, time_t *mtime);

inline int _wstat(const wchar_t *wcp_path, struct _stat64i32 *buf)
{
	if (buf == NULL)
		return -1;
	return _sfxFillStatWide(wcp_path, &buf->st_mode, &buf->st_size, &buf->st_mtime);
}

inline int _wstati64(const wchar_t *wcp_path, struct _stati64 *buf)
{
	if (buf == NULL)
		return -1;
	return _sfxFillStatWide(wcp_path, &buf->st_mode, &buf->st_size, &buf->st_mtime);
}

inline int _wunlink(const wchar_t *wcp_path)
{
	if (wcp_path == NULL)
		return -1;
	char buf[1024];
	size_t n = wcstombs(buf, wcp_path, sizeof(buf) - 1);
	if (n == (size_t)-1)
		return -1;
	buf[n] = 0;
	return unlink(buf);
}

inline const wchar_t *_wcserror(int error_number)
{
	static wchar_t wc_err[128];
	const char *msg = strerror(error_number);
	size_t n = mbstowcs(wc_err, msg ? msg : "", sizeof(wc_err) / sizeof(wc_err[0]) - 1);
	if (n == (size_t)-1)
		return L"error";
	wc_err[n] = 0;
	return wc_err;
}

/* glibc forbids mixing wide (fgetws) and narrow (fgets) operations on the
 * same FILE*. MSVC allowed it, and the .fac preset files are ANSI text on
 * disk, so fgetws is implemented here as a narrow read + multibyte→wide
 * conversion to keep stream orientation stable across all Vals* readers. */
inline wchar_t *fgetws_shim(wchar_t *dst, int dst_chars, FILE *stream)
{
	char buf[1024];
	if (dst_chars <= 0 || stream == NULL)
		return NULL;
	if (fgets(buf, (dst_chars > (int)sizeof(buf)) ? (int)sizeof(buf) : dst_chars, stream) == NULL)
		return NULL;
	size_t n = mbstowcs(dst, buf, (size_t)(dst_chars > 0 ? dst_chars - 1 : 0));
	if (n == (size_t)-1)
		return NULL;
	dst[n] = 0;
	return dst;
}
#define fgetws(dst, n, stream) fgetws_shim((dst), (n), (stream))
#endif /* __cplusplus */
#endif /* _PORT_STAT_SHIM */

#include <sys/stat.h>
#include <fcntl.h>
#include <ctype.h>
#include <wctype.h>

/* ------------------------------------------------------------------ */
/* Base types                                                          */
/* ------------------------------------------------------------------ */

typedef unsigned long DWORD;
typedef unsigned short WORD;
typedef unsigned char BYTE;
typedef void *HWND;
typedef void *HINSTANCE;
typedef void *HANDLE;
typedef void *LPVOID;
typedef void *LPSECURITY_ATTRIBUTES;
typedef void *PVOID;
typedef void *HMODULE;
typedef long LONG;
typedef long HRESULT;
typedef int BOOL;
typedef unsigned long UINT;
typedef unsigned long *LPDWORD;
typedef const char *LPCTSTR;
typedef const wchar_t *LPCWSTR;
typedef wchar_t *LPWSTR;
typedef const char *LPCSTR;
typedef char *LPTSTR;
typedef long WPARAM;
typedef long LPARAM;
typedef long LSTATUS;
typedef int errno_t;

#ifndef TRUE
#define TRUE 1
#define FALSE 0
#endif

#ifndef MAX_PATH
#define MAX_PATH 260
#endif

/* MSVC-only 64-bit integer keyword used by ptutil headers. */
#ifndef __int64
#define __int64 long long
#endif

/* ------------------------------------------------------------------ */
/* Legacy swprintf funnel (see file header note)                       */
/* ------------------------------------------------------------------ */

#ifdef __cplusplus
inline int _sfxLegacySwprintf(wchar_t *dst, const wchar_t *fmt, ...)
{
	va_list ap;
	va_start(ap, fmt);
	int result = vswprintf(dst, 1024, fmt, ap);
	va_end(ap);
	return result;
}
#define swprintf(dst, ...) _sfxLegacySwprintf((dst), __VA_ARGS__)
#define wsprintf(dst, ...) _sfxLegacySwprintf((dst), __VA_ARGS__)
#endif

/* http://msdn.microsoft.com msdn header: void Sleep(DWORD dwMilliseconds). */
static void Sleep(unsigned long dwMilliseconds)
{
	if (dwMilliseconds >= 20) {
		sleep(dwMilliseconds / 20);
		dwMilliseconds %= 20;
	}
	(void)dwMilliseconds; /* sub-20ms sleeps collapse to yield */
}

/* ------------------------------------------------------------------ */
/* MSVC CRT wide helpers, surveyed call sites only                     */
/* ------------------------------------------------------------------ */

#ifdef __cplusplus
inline int _wtoi(const wchar_t *wcp) { return (int)wcstol(wcp, NULL, 10); }
inline double _wtof(const wchar_t *wcp) { return wcstod(wcp, NULL); }
inline long _wtol(const wchar_t *wcp) { return wcstol(wcp, NULL, 10); }
inline wchar_t *wcstok_s(wchar_t *str, const wchar_t *delims, wchar_t **next)
{
	return wcstok(str, delims, next);
}

inline int _stricmp(const char *a, const char *b)
{
	while (*a != '\0' && *b != '\0') {
		int ca = (unsigned char)*a, cb = (unsigned char)*b;
		if (ca >= 'A' && ca <= 'Z') ca += 'a' - 'A';
		if (cb >= 'A' && cb <= 'Z') cb += 'a' - 'A';
		if (ca != cb)
			return ca - cb;
		a++, b++;
	}
	return (unsigned char)*a - (unsigned char)*b;
}

inline int _wcsicmp(const wchar_t *a, const wchar_t *b)
{
	while (*a != L'\0' && *b != L'\0') {
		int ca = (int)*a, cb = (int)*b;
		if (ca >= L'A' && ca <= L'Z') ca += L'a' - L'A';
		if (cb >= L'A' && cb <= L'Z') cb += L'a' - L'A';
		if (ca != cb)
			return ca - cb;
		a++, b++;
	}
	return (int)((int)*a - (int)*b);
}
#endif

/* MSVC secure-fopen (wide). */
#ifdef __cplusplus
inline int _wfopen_s(FILE **fp, const wchar_t *path, const wchar_t *mode)
{
	char mb_path[1024], mb_mode[16];
	if (path == NULL || wcstombs(mb_path, path, sizeof(mb_path)) == (size_t)-1)
		return 1;
	if (mode == NULL || wcstombs(mb_mode, mode, sizeof(mb_mode)) == (size_t)-1)
		return 1;
	*fp = fopen(mb_path, mb_mode);
	return *fp != NULL ? 0 : 1;
}
#endif


/* ------------------------------------------------------------------ */
/* Filetime + date/time (used by FILE/FileDate.cpp and file helpers)   */
/* ------------------------------------------------------------------ */

typedef struct _FILETIME {
	unsigned long dwLowDateTime;
	unsigned long dwHighDateTime;
} FILETIME;
typedef FILETIME *LPFILETIME;
typedef FILETIME *PFILETIME;

typedef struct _SYSTEMTIME {
	unsigned short wYear;
	unsigned short wMonth;
	unsigned short wDayOfWeek;
	unsigned short wDay;
	unsigned short wHour;
	unsigned short wMinute;
	unsigned short wSecond;
	unsigned short wMilliseconds;
} SYSTEMTIME;
typedef SYSTEMTIME *LPSYSTEMTIME;

typedef struct _TIME_ZONE_INFORMATION {
	long Bias;
} TIME_ZONE_INFORMATION;

#define LOCALE_SYSTEM_DEFAULT 0
#define LOCALE_NOUSEROVERRIDE 0

#ifdef __cplusplus
inline long long _sfxFiletimeToUnixMs(const FILETIME *ft)
{
	unsigned long long ticks = ((unsigned long long)ft->dwHighDateTime << 32) | ft->dwLowDateTime;
	long long unix_ms = (long long)(ticks / 10000ULL) - 11644473600000LL;
	return unix_ms < 0 ? 0 : unix_ms;
}

inline long long _sfxUnixMsToFiletime(long long unix_ms)
{
	return (unix_ms + 11644473600000LL) * 10000LL;
}

inline time_t _sfxTimegm(struct tm *tmv)
{
	long days;
	int y = tmv->tm_year + 1900, m = tmv->tm_mon + 1, d = tmv->tm_mday;
	if (m <= 2) {
		y -= 1;
		m += 12;
	}
	days = 365L * (y - 1970) + (y - 1970) / 4 - (y - 1970) / 100 + (y - 1970) / 400
	       + (153 * (m - 3) + 2) / 5 + d - 1;
	return (time_t)(days * 86400L + tmv->tm_hour * 3600L + tmv->tm_min * 60L + tmv->tm_sec);
}

inline long CompareFileTime(const FILETIME *a, const FILETIME *b)
{
	if (a == NULL || b == NULL)
		return 0;
	if (a->dwHighDateTime != b->dwHighDateTime)
		return a->dwHighDateTime < b->dwHighDateTime ? -1 : 1;
	if (a->dwLowDateTime != b->dwLowDateTime)
		return a->dwLowDateTime < b->dwLowDateTime ? -1 : 1;
	return 0;
}

inline void GetSystemTime(SYSTEMTIME *st)
{
	time_t now;
	time(&now);
	struct tm tmv;
	gmtime_r(&now, &tmv);
	st->wYear = (unsigned short)(1900 + tmv.tm_year);
	st->wMonth = (unsigned short)(tmv.tm_mon + 1);
	st->wDayOfWeek = (unsigned short)tmv.tm_wday;
	st->wDay = (unsigned short)tmv.tm_mday;
	st->wHour = (unsigned short)tmv.tm_hour;
	st->wMinute = (unsigned short)tmv.tm_min;
	st->wSecond = (unsigned short)tmv.tm_sec;
	st->wMilliseconds = 0;
}

/* Real 3-argument formatter for shim internals (bypasses the legacy
 * swprintf macro via vswprintf). */
inline int _sfxRawFormatW(wchar_t *dst, size_t n, const wchar_t *fmt, ...)
{
	va_list ap;
	va_start(ap, fmt);
	int result = vswprintf(dst, n, fmt, ap);
	va_end(ap);
	return result;
}

inline int FileTimeToSystemTime(const FILETIME *ft, SYSTEMTIME *st)
{
	long long ms = _sfxFiletimeToUnixMs(ft);
	time_t secs = (time_t)(ms / 1000);
	struct tm tmv;
	gmtime_r(&secs, &tmv);
	st->wYear = (unsigned short)(1900 + tmv.tm_year);
	st->wMonth = (unsigned short)(tmv.tm_mon + 1);
	st->wDayOfWeek = (unsigned short)tmv.tm_wday;
	st->wDay = (unsigned short)tmv.tm_mday;
	st->wHour = (unsigned short)tmv.tm_hour;
	st->wMinute = (unsigned short)tmv.tm_min;
	st->wSecond = (unsigned short)tmv.tm_sec;
	st->wMilliseconds = (unsigned short)(ms % 1000);
	return 1;
}

inline int SystemTimeToFileTime(const SYSTEMTIME *st, FILETIME *ft)
{
	struct tm tmv;
	memset(&tmv, 0, sizeof(tmv));
	tmv.tm_year = (st->wYear > 1900) ? st->wYear - 1900 : 70;
	tmv.tm_mon = st->wMonth > 0 ? st->wMonth - 1 : 0;
	tmv.tm_mday = st->wDay;
	tmv.tm_hour = st->wHour;
	tmv.tm_min = st->wMinute;
	tmv.tm_sec = st->wSecond;
	long long unix_ms = (long long)_sfxTimegm(&tmv) * 1000LL + st->wMilliseconds;
	long long ticks = _sfxUnixMsToFiletime(unix_ms);
	ft->dwLowDateTime = (unsigned long)((unsigned long long)ticks & 0xFFFFFFFFULL);
	ft->dwHighDateTime = (unsigned long)(((unsigned long long)ticks) >> 32);
	return 1;
}

inline unsigned long GetTimeZoneInformation(TIME_ZONE_INFORMATION *zone)
{
	if (zone != NULL)
		zone->Bias = 0;
	return 0 /* TIME_ZONE_ID_UNKNOWN */;
}

inline void SystemTimeToTzSpecificLocalTime(TIME_ZONE_INFORMATION *zone,
                                            const SYSTEMTIME *utc, SYSTEMTIME *local)
{
	(void)zone;
	if (local != NULL && utc != NULL)
		*local = *utc;
}

inline int GetDateFormatW(int locale, unsigned long flags, const SYSTEMTIME *st,
                          const wchar_t *format, wchar_t *buffer, int size)
{
	(void)locale;
	(void)flags;
	(void)format;
	if (buffer == NULL || st == NULL)
		return 0;
	int n = _sfxRawFormatW(buffer, 128, L"%d/%d/%d", (int)st->wMonth, (int)st->wDay, (int)st->wYear);
	return (n > 0 && n < size) ? n + 1 : 0;
}

inline int GetTimeFormatW(int locale, unsigned long flags, const SYSTEMTIME *st,
                          const wchar_t *format, wchar_t *buffer, int size)
{
	(void)locale;
	(void)flags;
	(void)format;
	if (buffer == NULL || st == NULL)
		return 0;
	int n = _sfxRawFormatW(buffer, 128, L"%02d:%02d:%02d", (int)st->wHour, (int)st->wMinute, (int)st->wSecond);
	return (n > 0 && n < size) ? n + 1 : 0;
}
#endif /* __cplusplus */

/* ------------------------------------------------------------------ */
/* Win32 file-handle surface (FILE/FileDate.cpp + File.h decls)        */
/* The Linux handle lives in src/shims/misc_linux.cpp and carries the */
/* FILE* plus the path so Get/SetFileTime can reach the on-disk node.  */
/* ------------------------------------------------------------------ */

#define GENERIC_READ   0x80000000UL
#define GENERIC_WRITE  0x40000000UL
#define FILE_SHARE_READ  0x00000001UL
#define FILE_SHARE_WRITE 0x00000002UL
#define CREATE_NEW       1
#define CREATE_ALWAYS    2
#define OPEN_EXISTING    3
#define OPEN_ALWAYS      4
#define TRUNCATE_EXISTING 5
#define FILE_ATTRIBUTE_NORMAL 0x80UL
#define INVALID_HANDLE_VALUE ((HANDLE)(long)(-1))

#ifdef __cplusplus
extern "C" {
#endif
void *_sfxOpenWin32File(const wchar_t *path, unsigned long access, unsigned long disposition,
                         unsigned long flags);
int _sfxCloseHandle(void *handle);
int _sfxGetFileTime(void *handle, void *created, void *accessed, void *modified);
int _sfxSetFileTime(void *handle, const void *created, const void *accessed, const void *modified);
int CloseHandle(void *handle);
int GetFileTime(void *handle, void *created, void *accessed, void *modified);
int SetFileTime(void *handle, const void *created, const void *accessed, const void *modified);
int MultiByteToWideChar(unsigned int codepage, unsigned long flags,
                        const char *src, int src_len, wchar_t *dst, int dst_chars);
int WideCharToMultiByte(unsigned int codepage, unsigned long flags,
                        const wchar_t *src, int src_len, char *dst, int dst_bytes,
                        const char *default_char, int *used_default);
int PathFileExistsW(const wchar_t *wcp_path);
#ifdef __cplusplus
}
#endif

/* ------------------------------------------------------------------ */
/* Directories, GUID, misc (pstr/FileGeneral/DfxDsp)                   */
/* ------------------------------------------------------------------ */

#ifdef __cplusplus
inline int CreateDirectoryW(const wchar_t *wcp_dir, void *attrs)
{
	(void)attrs;
	if (wcp_dir == NULL)
		return FALSE;
	char buf[1024];
	size_t n = wcstombs(buf, wcp_dir, sizeof(buf) - 1);
	if (n == (size_t)-1)
		return FALSE;
	buf[n] = 0;
	struct stat st;
	if (stat(buf, &st) == 0)
		return TRUE;
	return mkdir(buf, 0755) == 0 ? TRUE : FALSE;
}

inline int CreateDirectory(const char *cp_dir, void *attrs)
{
	(void)attrs;
	if (cp_dir == NULL)
		return FALSE;
	struct stat st;
	if (stat(cp_dir, &st) == 0)
		return TRUE;
	return mkdir(cp_dir, 0755) == 0 ? TRUE : FALSE;
}

#define FORMAT_MESSAGE_FROM_SYSTEM 0x00001000
inline unsigned long GetLastError() { return (unsigned long)(errno & 0xFFFF); }

inline unsigned long FormatMessage(unsigned long flags, const void *source,
                                   unsigned long message_id, unsigned long lang_id,
                                   wchar_t *wcp_buffer, unsigned long buffer_size, void *args)
{
	(void)flags;
	(void)source;
	(void)message_id;
	(void)lang_id;
	(void)args;
	if (wcp_buffer == NULL || buffer_size == 0)
		return 0;
	const wchar_t *msg = L"Operating system error";
	unsigned long i = 0;
	while (msg[i] != 0 && i + 1 < buffer_size) {
		wcp_buffer[i] = msg[i];
		i++;
	}
	wcp_buffer[i] = 0;
	return i;
}

#define MB_OK 0
inline int MessageBox(void *hwnd, const wchar_t *text, const wchar_t *caption, unsigned int type)
{
	(void)hwnd;
	(void)text;
	(void)type;
	fprintf(stderr, "MessageBox(no-op): %ls\n", caption ? caption : L"");
	return 0;
}

/* Library loading is only exercised by the Windows shared-memory backend
 * (dfxSharedUtil COMSFTWR shared libs); report failure and let the caller
 * take its "not loaded" path. */
inline void *LoadLibrary(const wchar_t *wcp_path) { (void)wcp_path; return NULL; }
inline int FreeLibrary(void *h) { (void)h; return 0; }

/* GUID support (only consumer: pstrGenerateGUID_Wide). */
#ifndef _PORT_GUID_DEFINED
#define _PORT_GUID_DEFINED
typedef struct _GUID {
	unsigned long  Data1;
	unsigned short Data2;
	unsigned short Data3;
	unsigned char  Data4[8];
} GUID;

#define S_OK 0
inline long CoCreateGuid(GUID *guid)
{
	if (guid == NULL)
		return 0x80004005L /* E_FAIL */;
	FILE *ur = fopen("/dev/urandom", "rb");
	if (ur != NULL) {
		if (fread(guid, 1, sizeof(*guid), ur) != sizeof(*guid)) {
			fclose(ur);
			return 0x80004005L;
		}
		fclose(ur);
	} else {
		unsigned long h = (unsigned long)(size_t)guid ^ (unsigned long)time(NULL);
		unsigned char *r = (unsigned char *)guid;
		for (size_t i = 0; i < sizeof(*guid); i++) {
			h = h * 1103515245UL + 12345UL;
			r[i] = (unsigned char)((h >> 16) & 0xFF);
		}
	}
	/* Mark RFC 4122 version 4 */
	guid->Data3 = (unsigned short)((guid->Data3 & 0x0FFF) | 0x4000);
	return S_OK;
}

inline long StringFromGUID2(GUID guid, wchar_t *wcp_guid, int i_buffer_length)
{
	if (wcp_guid == NULL || i_buffer_length < 39)
		return 0;
	int n = _sfxRawFormatW(wcp_guid, 1024,
	    L"{%08lX-%04X-%04X-%02X%02X-%02X%02X%02X%02X%02X%02X}",
	    (unsigned long)guid.Data1, guid.Data2, guid.Data3,
	    guid.Data4[0], guid.Data4[1], guid.Data4[2], guid.Data4[3],
	    guid.Data4[4], guid.Data4[5], guid.Data4[6], guid.Data4[7]);
	return n + 1;
}
#endif /* _PORT_GUID_DEFINED */

/* POSIX path splitting in the MSVC naming (FileGeneral.cpp). Linux paths
 * have no drive letters, so drive is always empty; directory keeps the
 * trailing slash like Windows' splitpath. */
#define _MAX_PATH   1024
#define _MAX_DRIVE  3
#define _MAX_DIR    256
#define _MAX_FNAME  256
#define _MAX_EXT    64

#ifdef __cplusplus
inline int _sfxWsplitpath(const wchar_t *path, wchar_t *drive, wchar_t *dir,
                           wchar_t *fname, wchar_t *ext)
{
	if (path == NULL)
		return -1;
	const wchar_t *slash = wcsrchr(path, '/');
	const wchar_t *bslash = wcsrchr(path, L'\\');
	const wchar_t *last_sep = (slash != NULL && bslash != NULL) ?
	                          (slash > bslash ? slash : bslash) :
	                          (slash != NULL ? slash : bslash);
	const wchar_t *end = path + wcslen(path);

	if (drive != NULL)
		drive[0] = 0; /* no drive letters on Linux */

	if (dir != NULL) {
		if (last_sep != NULL) {
			size_t n = (size_t)(last_sep - path) + 1;
			if (n >= (size_t)_MAX_DIR)
				n = _MAX_DIR - 1;
			wmemcpy(dir, path, n);
			dir[n] = 0;
		} else {
			dir[0] = 0;
		}
	}

	const wchar_t *base = (last_sep != NULL) ? last_sep + 1 : path;
	const wchar_t *dot = wcsrchr(base, L'.');
	if (ext != NULL) {
		if (dot != NULL) {
			wmemcpy(ext, dot, (size_t)(end - dot) + 1);
		} else {
			ext[0] = 0;
		}
	}
	if (fname != NULL) {
		size_t n = (dot != NULL) ? (size_t)(dot - base) : (size_t)(end - base);
		wmemcpy(fname, base, n);
		fname[n] = 0;
	}
	return 0;
}
#define _wsplitpath(path, drive, dir, fname, ext) \
	_sfxWsplitpath((path), (drive), (dir), (fname), (ext))

inline int _sfxFormatBoundedW(wchar_t *dst, size_t dst_chars, const wchar_t *fmt, ...)
{
	va_list ap;
	va_start(ap, fmt);
	int result = vswprintf(dst, dst_chars, fmt, ap);
	va_end(ap);
	return result;
}
#define swprintf_s(dst, size, ...) _sfxFormatBoundedW((dst), (size), __VA_ARGS__)

#endif /* __cplusplus */
#define CP_UTF8 65001
#define CP_ACP  0
#define MB_PRECOMPOSED 0x00000001

#endif /* __cplusplus guard falls through for pure-C consumers */

#endif /* _PORT_WINDOWS_SHIM_H */
