/*
 * SauceFX Linux app: Win32 compatibility shim.
 *
 * Minimal <mmsystem.h> replacement implementing only the multimedia
 * surface the PWAV module (ptutil/PWAV) actually references,
 * surveyed 2026-10:
 *
 *   Types:   WAVEFORMATEX, MMCKINFO, HMMIO, FOURCC, HPSTR, UINT
 *   Macros:  mmioFOURCC, WAVE_FORMAT_PCM, WAVE_FORMAT_QUERY,
 *            MMIO_READ/WRITE/CREATE/ALLOCBUF/FINDCHUNK/FINDRIFF/CREATERIFF
 *   Alloc:   LocalAlloc/Lock/Unlock/Free, GlobalAlloc/Lock/Unlock/Free
 *   IO:      mmioOpenW/Close/Read/Write/Descend/Ascend/CreateChunk
 *
 * mmio* is mapped onto POSIX stdio: chunk headers are raw RIFF records,
 * exactly as Windows would store/read them.
 */
#pragma once

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <windows.h>

typedef char *HPSTR;
typedef unsigned long FOURCC;
typedef unsigned long MMRESULT;
typedef void *HMMIO; /* points to a _PORT_MMIO_INFO */

#define MMIO_READ         0x00000000
#define MMIO_WRITE        0x00000001
#define MMIO_CREATE       0x00000800
#define MMIO_ALLOCBUF     0x00004000
#define MMIO_FINDCHUNK    0x00000010
#define MMIO_FINDRIFF     0x00000020
#define MMIO_CREATERIFF   0x00000020
/* Synthesized READ|WRITE mode for open(); Windows defines these bits
 * exclusively, READ is zero here so READWRITE is just MMIO_WRITE. */
#define MMIO_READWRITE    (MMIO_READ | MMIO_WRITE)

#define WAVE_FORMAT_PCM   1
#define WAVE_FORMAT_QUERY 0

#define MMIOERR_UNREACHABLE 0
#define MMIOERR_FILENOTFOUND 1

#ifndef MMERR_
#define MMIOERR_INVALID_FILE    2
#define MMIOERR_OUTOFMEMORY      3
#define MMIOERR_CANNOTOPEN       4
#define MMIOERR_CANNOTCLOSE      5
#define MMIOERR_CANNOTREAD       6
#define MMIOERR_CANNOTWRITE      7
#define MMIOERR_CANNOTSEEK       8
#define MMIOERR_CHUNK_NOT_FOUND 9
#endif


#ifndef _PORT_WAVEFORMATEX_DEFINED
#define _PORT_WAVEFORMATEX_DEFINED
typedef struct _WAVEFORMATEX {
	unsigned short wFormatTag;
	unsigned short nChannels;
	unsigned long nSamplesPerSec;
	unsigned long nAvgBytesPerSec;
	unsigned short nBlockAlign;
	unsigned short wBitsPerSample;
	unsigned short cbSize;
} WAVEFORMATEX;
typedef WAVEFORMATEX *PWAVEFORMATEX;
typedef WAVEFORMATEX *LPWAVEFORMATEX;
#endif

/* Windows waveOut/waveIn block header: only used as plain bookkeeping by
 * the Linux PWAV implementation (no OS wave device is touched). */
typedef struct _WAVEHDR {
	HPSTR lpData;
	unsigned long dwBufferLength;
	unsigned long dwBytesRecorded;
	unsigned long dwUser;
	unsigned long dwFlags;
	unsigned long dwLoops;
	unsigned long reserved;
} WAVEHDR;
typedef WAVEHDR *LPWAVEHDR;

#define WHDR_DONE      0x00000001
#define WHDR_PREPARED  0x00000002

/* Sound-card IO handles: kept only to preserve struct layout in pwav
 * state; never opened on the Linux DSP path. */
typedef void *HWAVEIN;
typedef void *HWAVEOUT;
typedef HWAVEIN *LPHWAVEIN;
typedef HWAVEOUT *LPHWAVEOUT;

typedef long LONG;
typedef unsigned long DWORD_PTR;

#define CALLBACK_WINDOW  0x00030000
#define CALLBACK_NULL    0x00000000
#define MMSYSERR_NOERROR      0
#define MMSYSERR_BADDEVICEID  2
#define MMSYSERR_NODRIVER     6

#define WAVE_IN_FAIL  MMSYSERR_BADDEVICEID
/*
 * No OS wave device exists on the Linux DSP path (input/output come from
 * the caller's buffers). Open requests fail, which pwav handles by
 * falling back; the other calls report success no-ops.
 */
inline MMRESULT waveInOpen(LPHWAVEIN, UINT, LPWAVEFORMATEX, DWORD_PTR, DWORD_PTR, unsigned long)
{ return MMSYSERR_BADDEVICEID; }
inline MMRESULT waveOutOpen(LPHWAVEOUT, UINT, LPWAVEFORMATEX, DWORD_PTR, DWORD_PTR, unsigned long)
{ return MMSYSERR_BADDEVICEID; }
inline MMRESULT waveInClose(HWAVEIN, ...) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutClose(HWAVEOUT, ...) { return MMSYSERR_NOERROR; }
inline MMRESULT waveInPrepareHeader(HWAVEIN, LPWAVEHDR, UINT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutPrepareHeader(HWAVEOUT, LPWAVEHDR, UINT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveInUnprepareHeader(HWAVEIN, LPWAVEHDR, UINT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutUnprepareHeader(HWAVEOUT, LPWAVEHDR, UINT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveInAddBuffer(HWAVEIN, LPWAVEHDR, UINT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutWrite(HWAVEOUT, LPWAVEHDR, UINT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveInStart(HWAVEIN) { return MMSYSERR_NOERROR; }
inline MMRESULT waveInReset(HWAVEIN) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutPause(HWAVEOUT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutRestart(HWAVEOUT) { return MMSYSERR_NOERROR; }
inline MMRESULT waveOutReset(HWAVEOUT) { return MMSYSERR_NOERROR; }

typedef struct _MMCKINFO {
	FOURCC ckid;
	unsigned long cksize;
	FOURCC fccType;
	unsigned long dwDataOffset;
	unsigned long dwFlags;
} MMCKINFO;
typedef MMCKINFO *LPMMCKINFO;

#define mmioFOURCC(ch0, ch1, ch2, ch3) \
	((FOURCC)((unsigned char)(ch0) | ((unsigned char)(ch1) << 8) | \
	          ((unsigned char)(ch2) << 16) | ((unsigned char)(ch3) << 24)))

/* ---- Alloc shim: Local and Global calls are plain heap allocations -- */

#define LMEM_FIXED 0x0000
#define LMEM_ZEROINIT 0x0040
#define LMEM_MOVEABLE 0x0002
#define GPTR (LPVOID)(LMEM_FIXED | LMEM_ZEROINIT)

/* Local/Global moveable handles do not exist on Linux heap; flag ignored. */
#define LOWORD(l) ((unsigned short)((l) & 0xFFFF))
#define HIWORD(l) ((unsigned short)(((l) >> 16) & 0xFFFF))

inline void *LocalAlloc(unsigned uFlags, unsigned uBytes)
{
	if (uFlags & LMEM_ZEROINIT) return calloc(1, uBytes);
	return malloc(uBytes);
}
inline void *LocalReAlloc(void *h, unsigned uBytes, unsigned uFlags)
{
	return realloc(h, uBytes);
}
inline void *LocalLock(void *h) { return h; }
/* Windows returns TRUE (non-zero) even when the handle is already unlocked
 * (no LMEM_LOCKCOUNT business on flat handles). */
inline int LocalUnlock(void *h) { return 1; }
inline void *LocalFree(void *h) { free(h); return NULL; }

typedef void *HGLOBAL;
#define GMEM_FIXED 0x0000
#define GMEM_ZEROINIT 0x0040
#define GMEM_MOVEABLE 0x0002
#define GMEM_SHARE 0x2000
inline HGLOBAL GlobalAlloc(unsigned uFlags, unsigned uBytes)
{
	if (uFlags & GMEM_ZEROINIT) return calloc(1, uBytes);
	return malloc(uBytes);
}
inline void *GlobalLock(HGLOBAL h) { return h; }
inline int GlobalUnlock(HGLOBAL h) { return 1; }
inline HGLOBAL GlobalFree(HGLOBAL h) { free(h); return NULL; }

/* ---- mmio shim ------------------------------------------------------- */

typedef struct _PORT_MMIO_INFO {
	FILE *fp;
	unsigned int open_flags;
} _PORT_MMIO_INFO;

#define _PORT_MMIO(_h) ((_PORT_MMIO_INFO *)(_h))

inline HMMIO _sfxMmioOpen(const char *cp_path, unsigned int flags)
{
	FILE *fp;
	if (flags & MMIO_CREATE)
		fp = fopen(cp_path, (flags & MMIO_READWRITE) ? "wb+" : "wb");
	else
		fp = fopen(cp_path, (flags & MMIO_READWRITE) ? "rb+" : "rb");
	if (fp == NULL)
		return (HMMIO)NULL;
	_PORT_MMIO_INFO *m = (_PORT_MMIO_INFO *)calloc(1, sizeof(_PORT_MMIO_INFO));
	if (m == NULL) { fclose(fp); return (HMMIO)NULL; }
	m->fp = fp;
	m->open_flags = flags;
	return (HMMIO)m;
}

inline HMMIO mmioOpenW(wchar_t *wcp_path, void *pmmioinfo, unsigned int flags)
{
	char buf[1024];
	size_t n = wcstombs(buf, wcp_path, sizeof(buf) - 1);
	if (n == (size_t)-1)
		return (HMMIO)NULL;
	buf[n] = 0;
	return _sfxMmioOpen(buf, flags);
}

/* Synthesized READ|WRITE mode for open(); Windows defines these bits
 * exclusively, READ is zero here so READWRITE is just MMIO_WRITE. */

inline MMRESULT mmioClose(HMMIO hmmio, unsigned int wFlags)
{
	if (hmmio == NULL)
		return 0;
	fclose(_PORT_MMIO(hmmio)->fp);
	hmmio = NULL;
	return 0;
}

inline long mmioRead(HMMIO hmmio, HPSTR pch, long cch)
{
	if (hmmio == NULL || pch == NULL || cch <= 0)
		return -1;
	return (long)fread(pch, 1, (size_t)cch, _PORT_MMIO(hmmio)->fp);
}

inline long mmioWrite(HMMIO hmmio, const char *pch, long cch)
{
	if (hmmio == NULL || pch == NULL || cch <= 0)
		return -1;
	return (long)fwrite(pch, 1, (size_t)cch, _PORT_MMIO(hmmio)->fp);
}

inline long _sfxMmioSeek(HMMIO hmmio, long offset, int origin)
{
	if (hmmio == NULL)
		return -1;
	return fseek(_PORT_MMIO(hmmio)->fp, offset, origin);
}

/*
 * Descend into a chunk. Windows semantics implemented:
 *  - flags 0 (MMIO_FINDCHUNK/FINDRIFF include): match pmmckinfo->ckid
 *    (for FINDRIFF, pmmckinfo->fccType is the searched RIFF form),
 *    scanning inside pmmckParentIn (or the whole file if NULL).
 *  - returns 0 on success; the input info is filled with the found chunk.
 */
inline MMRESULT mmioDescend(HMMIO hmmio, LPMMCKINFO info, const MMCKINFO *parent, unsigned int flags)
{
	if (hmmio == NULL || info == NULL)
		return MMIOERR_CANNOTREAD;

	FILE *fp = _PORT_MMIO(hmmio)->fp;
	long file_size, search_end, pos;
	if (fseek(fp, 0, SEEK_END) != 0)
		return MMIOERR_CANNOTSEEK;
	file_size = ftell(fp);
	if (file_size < 8)
		return MMIOERR_CHUNK_NOT_FOUND;

	if (parent != NULL)
		search_end = (long)(parent->dwDataOffset + parent->cksize);
	else
		search_end = file_size;
	if (search_end > file_size)
		search_end = file_size;

	FOURCC want = info->ckid;
	if (flags & MMIO_FINDRIFF)
		want = info->fccType;

	if (parent != NULL)
		pos = (long)parent->dwDataOffset + 8; /* past parent header */
	else
		pos = 0;

	while (pos + 8 <= search_end) {
		if (_sfxMmioSeek(hmmio, pos, SEEK_SET) != pos)
			return MMIOERR_CANNOTSEEK;
		FOURCC ckid;
		unsigned long cksize;
		if (fread(&ckid, 4, 1, fp) != 1 || fread(&cksize, 4, 1, fp) != 1)
			return MMIOERR_CANNOTREAD;
		if (ckid == want) {
			info->ckid = ckid;
			info->cksize = cksize;
			info->dwDataOffset = (unsigned long)(pos + 8);
			info->fccType = 0;
			if (ckid == mmioFOURCC('R','I','F','F') && pos + 12 <= search_end) {
				if (fread(&info->fccType, 4, 1, fp) != 1)
					return MMIOERR_CANNOTREAD;
			}
			/* Leave positioned at the chunk data start, like Windows. */
			_sfxMmioSeek(hmmio, (long)info->dwDataOffset + ((ckid == mmioFOURCC('R','I','F','F')) ? 4 : 0), SEEK_SET);
			return 0;
		}
		if (cksize & 1)
			cksize++; /* RIFF chunks are word-aligned */
		pos += 8 + (long)cksize;
	}
	return MMIOERR_CHUNK_NOT_FOUND;
}

/* Ascend past the current chunk; when writing, patch the size into the
 * header. Using dwDataOffset + cksize like Windows. */
inline MMRESULT mmioAscend(HMMIO hmmio, LPMMCKINFO info, unsigned int flags)
{
	if (hmmio == NULL || info == NULL)
		return MMIOERR_CANNOTREAD;

	if (_PORT_MMIO(hmmio)->open_flags & (MMIO_WRITE | MMIO_CREATE)) {
		long here = ftell(_PORT_MMIO(hmmio)->fp);
		long header = (long)info->dwDataOffset - 8;
		if (_sfxMmioSeek(hmmio, header, SEEK_SET) == header) {
			fwrite(&info->ckid, 4, 1, _PORT_MMIO(hmmio)->fp);
			fwrite(&info->cksize, 4, 1, _PORT_MMIO(hmmio)->fp);
			_sfxMmioSeek(hmmio, here, SEEK_SET);
		}
	}
	/* Position after the chunk data (+ pad byte) for the next header. */
	_sfxMmioSeek(hmmio, (long)(info->dwDataOffset + info->cksize + (info->cksize & 1)), SEEK_SET);
	return 0;
}

/* Create a chunk header at the current position and leave positioned at
 * the data start (Windows MMIO_CREATELIST/RIFF semantics covered: when
 * info->ckid is RIFF/WAVE-list form, the fourcc form is appended). */
inline MMRESULT mmioCreateChunk(HMMIO hmmio, LPMMCKINFO info, unsigned int flags)
{
	if (hmmio == NULL || info == NULL)
		return MMIOERR_CANNOTREAD;

	FILE *fp = _PORT_MMIO(hmmio)->fp;
	if (fwrite(&info->ckid, 4, 1, fp) != 1)
		return MMIOERR_CANNOTWRITE;
	if (fwrite(&info->cksize, 4, 1, fp) != 1)
		return MMIOERR_CANNOTWRITE;
	long data_pos = ftell(fp);
	info->dwDataOffset = (unsigned long)data_pos;
	if (info->ckid == mmioFOURCC('R','I','F','F') || info->ckid == mmioFOURCC('L','I','S','T')) {
		if (fwrite(&info->fccType, 4, 1, fp) != 1)
			return MMIOERR_CANNOTWRITE;
		info->dwDataOffset += 4;
	}
	return 0;
}
