/*
 * SauceFX Linux app. POSIX implementations for the Windows
 * helper functions the dsp helper modules call (removed from their shell
 * APIs via stub headers).
 */

#include <Shlwapi.h>
#include <windows.h>

#include <direct.h>
#include "slout.h"

#include <cwchar>
#include <cstdio>
#include <cstring>
#include <string>
#include <sys/stat.h>
#include <fcntl.h> /* AT_FDCWD */
#include <sys/stat.h> /* utimensat */

/* Shlwapi.h: FILE/FileGeneral.cpp fileExist_Wide(). */
extern "C" int PathFileExistsW(const wchar_t *wcp_path)
{
	if (wcp_path == NULL)
		return 0;
	char buf[1024];
	size_t n = wcstombs(buf, wcp_path, sizeof(buf) - 1);
	if (n == (size_t)-1)
		return 0;
	buf[n] = 0;
	struct stat st;
	return stat(buf, &st) == 0;
}

/* direct.h equivalents used by FILE helpers. */
extern "C" int _mkdir(const char *cp_path)
{
	return mkdir(cp_path, 0755);
}

/*
 * Win32 versus Windows file-handle translations for FileDate.cpp: open,
 * read/modify mtime, close. The handle is a heap _PORT_WIN32_FILE.
 */
struct _PORT_WIN32_FILE {
	FILE *fp;
	char path[1024];
};

extern "C" void *_sfxOpenWin32File(const wchar_t *path, unsigned long access,
                                    unsigned long disposition, unsigned long flags)
{
	(void)flags;
	if (path == NULL)
		return INVALID_HANDLE_VALUE;
	char buf[1024];
	size_t n = wcstombs(buf, path, sizeof(buf) - 1);
	if (n == (size_t)-1)
		return INVALID_HANDLE_VALUE;
	buf[n] = 0;

	const char *mode;
	bool for_write = (access & GENERIC_WRITE) != 0;
	struct stat st;
	bool file_exists = (stat(buf, &st) == 0);
	if (file_exists) {
		if (disposition == CREATE_NEW)
			return INVALID_HANDLE_VALUE;
		mode = for_write ? "rb+" : "rb";
	} else {
		if (disposition == OPEN_EXISTING || disposition == TRUNCATE_EXISTING)
			return INVALID_HANDLE_VALUE;
		mode = "wb+";
	}
	FILE *fp = fopen(buf, mode);
	if (fp == NULL)
		return INVALID_HANDLE_VALUE;

	_PORT_WIN32_FILE *h = (_PORT_WIN32_FILE *)calloc(1, sizeof(_PORT_WIN32_FILE));
	if (h == NULL) {
		fclose(fp);
		return INVALID_HANDLE_VALUE;
	}
	h->fp = fp;
	strncpy(h->path, buf, sizeof(h->path) - 1);
	return h;
}

extern "C" int _sfxGetFileTime(void *handle, void *created, void *accessed, void *modified)
{
	_PORT_WIN32_FILE *h = (_PORT_WIN32_FILE *)handle;
	if (h == NULL)
		return 0;
	struct stat st;
	if (stat(h->path, &st) != 0)
		return 0;
	unsigned long long ticks = ((unsigned long long)st.st_mtime + 11644473600ULL) * 10000000ULL
	                           + ((unsigned long long)st.st_mtim.tv_nsec / 100ULL);
	FILETIME tmp;
	tmp.dwLowDateTime = (unsigned long)(ticks & 0xFFFFFFFFULL);
	tmp.dwHighDateTime = (unsigned long)(ticks >> 32);
	if (created != NULL)
		memcpy(created, &tmp, sizeof(tmp));
	if (accessed != NULL)
		memcpy(accessed, &tmp, sizeof(tmp));
	if (modified != NULL)
		memcpy(modified, &tmp, sizeof(tmp));
	return 1;
}

extern "C" int _sfxSetFileTime(void *handle, const void *created, const void *accessed, const void *modified)
{
	_PORT_WIN32_FILE *h = (_PORT_WIN32_FILE *)handle;
	if (h == NULL)
		return 0;
	const FILETIME *ft = (const FILETIME *)(modified != NULL ? modified : created);
	if (ft == NULL)
		return 1;
	unsigned long long ticks = ((unsigned long long)ft->dwHighDateTime << 32) | ft->dwLowDateTime;
	long long unix_ms = (long long)(ticks / 10000ULL) - 11644473600000LL;
	if (unix_ms < 0)
		unix_ms = 0;
	struct timespec ts[2];
	ts[0].tv_sec = (time_t)(unix_ms / 1000);
	ts[0].tv_nsec = (long)((unix_ms % 1000) * 1000000L);
	ts[1] = ts[0];
	return utimensat(AT_FDCWD, h->path, ts, 0) == 0 ? 1 : 0;
}

extern "C" int _sfxCloseHandle(void *handle)
{
	_PORT_WIN32_FILE *h = (_PORT_WIN32_FILE *)handle;
	if (h == NULL)
		return 0;
	if (h->fp != NULL)
		fclose(h->fp);
	free(h);
	return 1;
}

extern "C" int CloseHandle(void *handle) { return _sfxCloseHandle(handle); }
extern "C" int GetFileTime(void *handle, void *created, void *accessed, void *modified)
{
	return _sfxGetFileTime(handle, created, accessed, modified);
}
extern "C" int SetFileTime(void *handle, const void *created, const void *accessed, const void *modified)
{
	return _sfxSetFileTime(handle, created, accessed, modified);
}

/* dsp/ptutil File.h wrapper — replaces the upstream Win32 file-handle layer.
 * No extern "C": File.h declares it C++-mangled. */
void *fileWin32CreateFile_Wide(const wchar_t *wcp_file_name, unsigned long dw_desired_access,
                               unsigned long dw_share_mode, void *attrs,
                               unsigned long dw_creation_disposition, unsigned long dw_flags,
                               void *h_template, CSlout *hp_slout)
{
	(void)attrs;
	(void)dw_flags;
	(void)h_template;
	(void)dw_share_mode;
	(void)hp_slout;
	return _sfxOpenWin32File(wcp_file_name, dw_desired_access, dw_creation_disposition, 0);
}

/* POSIX stat in MSVC naming; see the _PORT_STAT_SHIM block in
 * src/include/windows.h. Defined here where sys/stat.h is complete. */
extern "C" int _sfxFillStatWide(const wchar_t *wcp_path, unsigned long *mode,
                                 long long *size, time_t *mtime)
{
	if (wcp_path == NULL)
		return -1;
	char buf[1024];
	size_t n = wcstombs(buf, wcp_path, sizeof(buf) - 1);
	if (n == (size_t)-1)
		return -1;
	buf[n] = 0;
	struct stat st;
	if (stat(buf, &st) != 0)
		return -1;
	if (mode != NULL)
		*mode = st.st_mode;
	if (size != NULL)
		*size = (long long)st.st_size;
	if (mtime != NULL)
		*mtime = (time_t)st.st_mtime;
	return 0;
}
extern "C" int MultiByteToWideChar(unsigned int codepage, unsigned long flags,
                                   const char *src, int src_len, wchar_t *dst, int dst_chars)
{
	(void)codepage;
	(void)flags;
	if (src == NULL)
		return 0;

	size_t n = (src_len < 0) ? strlen(src) : (size_t)src_len;

	/* Count pass */
	if (dst == NULL) {
		int count = 0;
		for (size_t i = 0; i < n;) {
			unsigned char c = (unsigned char)src[i];
			int extra;
			if (c < 0x80) extra = 0;
			else if ((c & 0xE0) == 0xC0) extra = 1;
			else if ((c & 0xF0) == 0xE0) extra = 2;
			else if ((c & 0xF8) == 0xF0) extra = 3;
			else return 0;
			unsigned long cp = c & (extra == 0 ? 0x7F : extra == 1 ? 0x1F : extra == 2 ? 0x0F : 0x07);
			for (int k = 1; k <= extra; k++) {
				if (i + (size_t)k >= n || ((unsigned char)src[i + k] & 0xC0) != 0x80)
					return 0;
				cp = (cp << 6) | ((unsigned char)src[i + k] & 0x3F);
			}
			i += 1 + (size_t)extra;
			if (cp >= 0x10000)
				count += 2;
			else
				count += 1;
		}
		if (src_len < 0)
			count += 1; /* room for terminator */
		return count;
	}

	/* Fill pass */
	int produced = 0;
	for (size_t i = 0; i < n;) {
		unsigned char c = (unsigned char)src[i];
		int extra;
		if (c < 0x80) extra = 0;
		else if ((c & 0xE0) == 0xC0) extra = 1;
		else if ((c & 0xF0) == 0xE0) extra = 2;
		else if ((c & 0xF8) == 0xF0) extra = 3;
		else return 0;
		unsigned long cp = c & (extra == 0 ? 0x7F : extra == 1 ? 0x1F : extra == 2 ? 0x0F : 0x07);
		for (int k = 1; k <= extra; k++) {
			if (i + (size_t)k >= n || ((unsigned char)src[i + k] & 0xC0) != 0x80)
				return 0;
			cp = (cp << 6) | ((unsigned char)src[i + k] & 0x3F);
		}
		i += 1 + (size_t)extra;

		if (cp >= 0x10000) {
			if (produced + 2 > dst_chars)
				return 0;
			cp -= 0x10000;
			dst[produced++] = (wchar_t)(0xD800 | (cp >> 10));
			dst[produced++] = (wchar_t)(0xDC00 | (cp & 0x3FF));
		} else if (cp >= 0xD800 && cp <= 0xDFFF) {
			return 0;
		} else {
			if (produced + 1 > dst_chars)
				return 0;
			dst[produced++] = (wchar_t)cp;
		}
	}
	if (src_len < 0) {
		if (produced + 1 > dst_chars)
			return 0;
		dst[produced++] = 0;
	}
	return produced;
}

extern "C" int WideCharToMultiByte(unsigned int codepage, unsigned long flags,
                                   const wchar_t *src, int src_len, char *dst, int dst_bytes,
                                   const char *default_char, int *used_default)
{
	(void)codepage;
	(void)flags;
	(void)default_char;
	if (used_default != NULL)
		*used_default = 0;
	if (src == NULL)
		return 0;

	size_t wlen = (src_len < 0) ? wcslen(src) : (size_t)src_len;

	/* Count pass */
	if (dst == NULL) {
		size_t count = 0;
		for (size_t i = 0; i < wlen; i++) {
			unsigned long cp = src[i];
			if (cp >= 0xD800 && cp <= 0xDBFF && i + 1 < wlen &&
			    src[i + 1] >= 0xDC00 && src[i + 1] <= 0xDFFF) {
				cp = 0x10000 + ((cp - 0xD800) << 10) + (src[i + 1] - 0xDC00);
				i++;
			}
			if (cp < 0x80) count += 1;
			else if (cp < 0x800) count += 2;
			else if (cp < 0x10000) count += 3;
			else count += 4;
		}
		if (src_len < 0)
			count += 1;
		return (int)count;
	}

	int produced = 0;
	for (size_t i = 0; i < wlen; i++) {
		unsigned long cp = src[i];
		if (cp >= 0xD800 && cp <= 0xDBFF && i + 1 < wlen &&
		    src[i + 1] >= 0xDC00 && src[i + 1] <= 0xDFFF) {
			cp = 0x10000 + ((cp - 0xD800) << 10) + (src[i + 1] - 0xDC00);
			i++;
		}
		unsigned char buf[4];
		int bytes;
		if (cp < 0x80) { buf[0] = (unsigned char)cp; bytes = 1; }
		else if (cp < 0x800) {
			buf[0] = (unsigned char)(0xC0 | (cp >> 6));
			buf[1] = (unsigned char)(0x80 | (cp & 0x3F));
			bytes = 2;
		} else if (cp < 0x10000) {
			buf[0] = (unsigned char)(0xE0 | (cp >> 12));
			buf[1] = (unsigned char)(0x80 | ((cp >> 6) & 0x3F));
			buf[2] = (unsigned char)(0x80 | (cp & 0x3F));
			bytes = 3;
		} else {
			buf[0] = (unsigned char)(0xF0 | (cp >> 18));
			buf[1] = (unsigned char)(0x80 | ((cp >> 12) & 0x3F));
			buf[2] = (unsigned char)(0x80 | ((cp >> 6) & 0x3F));
			buf[3] = (unsigned char)(0x80 | (cp & 0x3F));
			bytes = 4;
		}
		if (produced + bytes > dst_bytes)
			return 0;
		memcpy(dst + produced, buf, (size_t)bytes);
		produced += bytes;
	}
	if (src_len < 0) {
		if (produced + 1 > dst_bytes)
			return 0;
		dst[produced++] = 0;
	}
	return produced;
}
