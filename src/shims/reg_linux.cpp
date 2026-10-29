/*
 * SauceFX Linux app. File-backed POSIX replacement for the
 * Windows registry layer (audiopassthru/src/reg/*, which is Win32-only).
 *
 * Surveyed 2026-10: the DSP core only ever calls regReadKey_Wide() and
 * regCreateKey_Wide(). The other reg.h functions are provided so the
 * library links, but they stay simple by design.
 *
 * Storage: $FXSOUND_REG_FILE, else $XDG_STATE_HOME, else $HOME in
 * ".local/state/saucefx/registry.txt". One "root\\key\t value" line per
 * entry, UTF-8 encoded. Survives restarts; behaves like HKCU.
 */

#include "reg.h"
#include "codedefs.h"

#include <cstdio>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include <string>
#include <vector>
#include <fstream>
#include <sstream>
#include <mutex>
#include <sys/stat.h>
#include <unistd.h>

namespace {

struct RegEntry {
	std::wstring key;
	std::wstring value;
};

std::wstring regFile()
{
	const char *env = getenv("FXSOUND_REG_FILE");
	if (env != NULL)
		return std::wstring(env, env + strlen(env));

	std::string dir;
	env = getenv("XDG_STATE_HOME");
	if (env != NULL)
		dir = env;
	else {
		env = getenv("HOME");
		dir = (env != NULL) ? env : ".";
		dir += "/.local/state";
	}
	return std::wstring(dir.begin(), dir.end()) + L"/saucefx/registry.txt";
}

bool readStore(const std::wstring &path, std::vector<RegEntry> *entries)
{
	std::ifstream in(std::string(path.begin(), path.end()));
	entries->clear();
	std::string line;
	while (std::getline(in, line)) {
		size_t tab = line.find('\t');
		if (tab == std::string::npos)
			continue;
		std::string ukey = line.substr(0, tab);
		std::string uval = line.substr(tab + 1);
		entries->push_back({std::wstring(ukey.begin(), ukey.end()),
		                    std::wstring(uval.begin(), uval.end())});
	}
	return true;
}

bool writeStore(const std::wstring &path, const std::vector<RegEntry> &entries)
{
	std::string sp(path.begin(), path.end());
	size_t slash = sp.find_last_of('/');
	if (slash != std::string::npos) {
		std::string dir = sp.substr(0, slash);
		mkdir(dir.c_str(), 0755);
	}

	/* Write a sibling file and rename it over the real one. The rename is
	 * atomic, so a reader (the DSP audio thread calls this via
	 * eqUpdateFromRegistry()) sees either the old complete file or the new
	 * complete file, never a truncated one. An in-place truncate/rewrite
	 * used to let a concurrent read miss every band still to be written;
	 * dfxpEqGetBandBoostCut_FromRegistry() treats a missing key as 0 dB and
	 * then clobbers the live gains. */
	std::string tmp = sp + ".tmp." + std::to_string((long)getpid());
	std::ofstream out(tmp, std::ios::trunc);
	if (!out.is_open())
		return false;
	for (const RegEntry &e : entries) {
		out << std::string(e.key.begin(), e.key.end()) << '\t'
		    << std::string(e.value.begin(), e.value.end()) << '\n';
	}
	out.flush();
	bool written = out.good();
	out.close();
	if (!written || std::rename(tmp.c_str(), sp.c_str()) != 0) {
		std::remove(tmp.c_str());
		return false;
	}
	return true;
}

/* regCreateKey_Wide/regReadKey_Wide are called from both the GUI thread and
 * the PipeWire audio thread, and each write is a read-modify-write of the
 * whole file. Serialize the store so those threads cannot lose each other's
 * updates. */
std::mutex &storeMutex()
{
	static std::mutex mutex;
	return mutex;
}

int upsertOne(int root, const std::wstring &key, const std::wstring &value)
{
	std::lock_guard<std::mutex> lock(storeMutex());
	std::vector<RegEntry> entries;
	std::wstring path = regFile();
	readStore(path, &entries);

	std::wstring full = std::to_wstring(root) + L"|" + key;
	bool found = false;
	for (RegEntry &e : entries) {
		if (e.key == full) {
			e.value = value;
			found = true;
			break;
		}
	}
	if (!found)
		entries.push_back({full, value});
	return writeStore(path, entries) ? OKAY : NOT_OKAY;
}

bool lookup(int root, const std::wstring &key, std::wstring *value)
{
	std::lock_guard<std::mutex> lock(storeMutex());
	std::vector<RegEntry> entries;
	readStore(regFile(), &entries);

	std::wstring full = std::to_wstring(root) + L"|" + key;
	for (const RegEntry &e : entries) {
		if (e.key == full) {
			*value = e.value;
			return true;
		}
	}
	return false;
}

} /* namespace */

/* regWithoutKeyname.cpp */

int PT_DECLSPEC regCreateKey_Wide(int root, wchar_t *wcp_key_path, wchar_t *wcp_value)
{
	if (wcp_key_path == NULL)
		return NOT_OKAY;
	return upsertOne(root, std::wstring(wcp_key_path), wcp_value ? std::wstring(wcp_value) : L"");
}

int PT_DECLSPEC regReadKey_Wide(int root, wchar_t *wcp_key, int *ip_value_exists,
                                wchar_t *wcp_value, unsigned long ul_value_buffer_length)
{
	if (ip_value_exists == NULL || wcp_key == NULL)
		return NOT_OKAY;
	*ip_value_exists = FALSE;
	if (wcp_value != NULL && ul_value_buffer_length > 0)
		wcp_value[0] = 0;

	std::wstring value;
	if (!lookup(root, std::wstring(wcp_key), &value))
		return OKAY;

	*ip_value_exists = TRUE;
	if (wcp_value != NULL && ul_value_buffer_length > 0) {
		unsigned long n = ul_value_buffer_length - 1;
		if (n > value.size())
			n = value.size();
		wmemcpy(wcp_value, value.c_str(), n);
		wcp_value[n] = 0;
	}
	return OKAY;
}

/* --- Low-usage reg.h surface: compile-compatible thin versions ------- */

int PT_DECLSPEC regReadTopDir_Wide(wchar_t *wcp_dir, int i_num_dirs, int io_major,
                                   int io_minor, CSlout *hp_slout)
{
	return NOT_OKAY;
}

int PT_DECLSPEC regReadRegisteredOwner(char *cp_owner, int i_flag) { return NOT_OKAY; }
int PT_DECLSPEC regReadRegisteredOwner_Wide(wchar_t *wcp_owner, int i_flag) { return NOT_OKAY; }

int PT_DECLSPEC regRemoveKey(int root, char *cp_key) { return OKAY; }
int PT_DECLSPEC regRemoveKey_Wide(int root, wchar_t *wcp_key) { return OKAY; }

int PT_DECLSPEC regCreateKeyTest_Wide(int root, wchar_t *wcp_key, wchar_t *wcp_value, int *ip_test_result)
{
	if (ip_test_result != NULL)
		*ip_test_result = 1;
	(void)root;
	(void)wcp_key;
	(void)wcp_value;
	return OKAY;
}

int PT_DECLSPEC regCreateKeyWithKeyname_Dword_Wide(int root, wchar_t *wcp_key_path,
                                                   wchar_t *wcp_keyname, unsigned long ul_value)
{
	std::wstring full = std::wstring(wcp_key_path ? wcp_key_path : L"") + L"\\" +
	                    std::wstring(wcp_keyname ? wcp_keyname : L"");
	return upsertOne(root, full, std::to_wstring((long long)ul_value));
}

int PT_DECLSPEC regCreateKeyWithKeyname_String_Wide(int root, wchar_t *wcp_key_path,
                                                    wchar_t *wcp_keyname, wchar_t *wcp_value)
{
	std::wstring full = std::wstring(wcp_key_path ? wcp_key_path : L"") + L"\\" +
	                    std::wstring(wcp_keyname ? wcp_keyname : L"");
	return upsertOne(root, full, wcp_value ? std::wstring(wcp_value) : L"");
}

int PT_DECLSPEC regReadKeyWithKeyname_String_Wide(int root, wchar_t *wcp_key_path,
                                                  wchar_t *wcp_keyname, int *ip_value_exists,
                                                  wchar_t *wcp_value, unsigned long ul_buffer_length)
{
	std::wstring full = std::wstring(wcp_key_path ? wcp_key_path : L"") + L"\\" +
	                    std::wstring(wcp_keyname ? wcp_keyname : L"");
	return regReadKey_Wide(root, full.empty() ? NULL : &full[0], ip_value_exists,
	                       wcp_value, ul_buffer_length);
}

int PT_DECLSPEC regReadKeyWithKeyname_Dword_Wide(int root, wchar_t *wcp_key_path,
                                                 wchar_t *wcp_keyname, int *ip_value_exists,
                                                 unsigned long *rp_value)
{
	std::wstring full = std::wstring(wcp_key_path ? wcp_key_path : L"") + L"\\" +
	                    std::wstring(wcp_keyname ? wcp_keyname : L"");
	if (ip_value_exists != NULL)
		*ip_value_exists = FALSE;

	std::wstring value;
	if (!lookup(root, full, &value))
		return OKAY;
	*ip_value_exists = TRUE;
	if (rp_value != NULL) {
		try {
			*rp_value = (unsigned long)std::stoll(value);
		} catch (...) {
			*rp_value = 0;
		}
	}
	return OKAY;
}

int PT_DECLSPEC regRecursiveDeleteFolder_Wide(int root, wchar_t *wcp_folder)
{
	/* The file store has no hierarchy to recurse; treat as no-op. */
	(void)root;
	(void)wcp_folder;
	return OKAY;
}
