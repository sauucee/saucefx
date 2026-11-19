Name:           saucefx
Version:        0.2.4
Release:        1%{?dist}
Summary:        Sound EQ and audio enhancement for Linux (native Rust/Slint UI)
License:        AGPL-3.0-only
URL:            https://github.com/sauucee/saucefx
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  gcc-c++ make rust cargo clang llvm-devel ninja-build
BuildRequires:  pipewire-devel pulseaudio-libs-devel fontconfig-devel
BuildRequires:  freetype-devel libxkbcommon-devel mesa-libGL-devel
BuildRequires:  mesa-libEGL-devel desktop-file-utils libappstream-glib
Requires:       pipewire pipewire-pulseaudio pulseaudio-libs zenity
Requires:       fontconfig freetype libxkbcommon mesa-libGL mesa-libEGL
Requires:       hicolor-icon-theme

%description
SauceFX is a system-wide sound EQ and audio enhancement tool for Linux.
This package ships the native Rust/Slint interface over the same DSP
engine and PipeWire bridge as the original application.

%prep
%setup -q

%build
make -C src rt %{?_smp_mflags}
CARGO_BUILD_JOBS=2 cargo build --release -p saucefx-host \
    --no-default-features --features renderer-skia

%install
install -Dm0755 target/release/saucefx %{buildroot}%{_bindir}/SauceFX
install -d %{buildroot}%{_datadir}/saucefx
cp -a presets %{buildroot}%{_datadir}/saucefx/presets
install -Dm0644 packaging/linux/io.github.sauucee.SauceFX.desktop \
    %{buildroot}%{_datadir}/applications/io.github.sauucee.SauceFX.desktop
install -Dm0644 packaging/linux/io.github.sauucee.SauceFX.metainfo.xml \
    %{buildroot}%{_datadir}/metainfo/io.github.sauucee.SauceFX.metainfo.xml
install -Dm0644 assets/Images/saucefx-logo-512.png \
    %{buildroot}%{_datadir}/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png

%check
desktop-file-validate \
    %{buildroot}%{_datadir}/applications/io.github.sauucee.SauceFX.desktop
appstreamcli validate --no-net \
    %{buildroot}%{_datadir}/metainfo/io.github.sauucee.SauceFX.metainfo.xml

%files
%{_bindir}/SauceFX
%{_datadir}/saucefx
%{_datadir}/applications/io.github.sauucee.SauceFX.desktop
%{_datadir}/metainfo/io.github.sauucee.SauceFX.metainfo.xml
%{_datadir}/icons/hicolor/512x512/apps/io.github.sauucee.SauceFX.png
%license LICENSE

%changelog
* Wed Nov 19 2026 SauceFX <sauucee@users.noreply.github.com> - 0.2.4-1
- Native Rust/Slint UI package.