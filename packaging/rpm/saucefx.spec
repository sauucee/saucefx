%global debug_package %{nil}
%global _hardened_build 0

Name:           saucefx
Version:        0.2.2
Release:        1%{?dist}
Summary:        Sound EQ and audio enhancement for Linux
License:        AGPL-3.0-or-later
URL:            https://github.com/sauucee/saucefx
BuildArch:      %{_arch}
Requires:       hicolor-icon-theme
Requires:       /usr/bin/pactl
Requires:       pipewire-pulseaudio
# JUCE loads the browser backend with dlopen(), so it does not appear in the
# automatically generated library dependencies. WebKitGTK already resolves
# its GTK3 and libsoup3 dependencies through DNF.
Requires:       webkit2gtk4.1

%description
SauceFX is a native Linux sound equalizer and audio enhancement app built
around the open-source DfxDsp engine and PipeWire. It provides a five-effect
rack, a 31-band graphic EQ with a live spectrum, presets, themes, session
restore, and desktop integration.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a %{_saucefx_root}/. %{buildroot}/

%posttrans
update-desktop-database -q %{_datadir}/applications 2>/dev/null || :
gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor 2>/dev/null || :

%postun
update-desktop-database -q %{_datadir}/applications 2>/dev/null || :
gtk-update-icon-cache -q -t -f %{_datadir}/icons/hicolor 2>/dev/null || :

%files
%{_bindir}/SauceFX
%{_datadir}/saucefx
%{_datadir}/applications/io.github.sauucee.SauceFX.desktop
%{_datadir}/metainfo/io.github.sauucee.SauceFX.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/io.github.sauucee.SauceFX.svg
%{_datadir}/icons/hicolor/128x128/apps/io.github.sauucee.SauceFX.png
%{_datadir}/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png
%license %{_datadir}/licenses/saucefx/LICENSE
%doc %{_datadir}/doc/saucefx/README.md

%changelog
* Mon Nov 02 2026 SauceFX <sauucee@users.noreply.github.com> - 0.2.2-1
- Discover physical audio outputs and recover routing after device changes.

* Sat Oct 31 2026 SauceFX <sauucee@users.noreply.github.com> - 0.2.1-1
- Update desktop identity, metadata, and package integration.

* Wed Oct 21 2026 SauceFX <noreply@saucefx.local> - 0.2.0-1
- Initial packaged release.
