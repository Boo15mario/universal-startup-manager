Name:           universal-startup-manager
Version:        0.1.0
Release:        1%{?dist}
Summary:        GTK4 app to manage XDG autostart entries

License:        GPL-3.0-or-later
URL:            https://github.com/Boo15mario/universal-startup-manager
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  rust-packaging
BuildRequires:  pkgconfig(gio-2.0)
BuildRequires:  pkgconfig(glib-2.0)
BuildRequires:  pkgconfig(gtk4)

%description
Universal Startup Manager is a GTK4 Rust application that reads user and system
XDG autostart entries and lets you filter, toggle, add, edit, or delete
user-owned entries while keeping system entries read-only. It preserves extra
.desktop keys and localized names when rewriting files.

%prep
%autosetup -n %{name}-%{version}
%cargo_prep

%build
%cargo_build --release

%install
%cargo_install

%check
%cargo_test --release

%files
%license LICENSE
%doc README.md
%{_bindir}/universal-startup-manager

%changelog
* Sun Dec 14 2025 Your Name <you@example.com> - 0.1.0-1
- Initial RPM release
