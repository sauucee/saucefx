/*
 * SauceFX Linux app. MSVC <Shlwapi.h> stub — FileGeneral uses
 * PathFileExistsW; implemented inline on POSIX access().
 */
#pragma once

#ifdef __cplusplus
extern "C" int PathFileExistsW(const wchar_t *wcp_path);
#else
int PathFileExistsW(const wchar_t *wcp_path);
#endif
