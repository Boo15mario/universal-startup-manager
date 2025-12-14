Name:           universal-startup-manager
Version:        1.0.2
Release:        1%{?dist}
Summary:        GTK4 app to manage XDG autostart entries

License:        GPL-3.0-or-later
URL:            https://github.com/Boo15mario/universal-startup-manager
Source0:        %{url}/archive/refs/tags/v%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconfig(gio-2.0)
BuildRequires:  pkgconfig(glib-2.0)
BuildRequires:  pkgconfig(gtk4)
BuildRequires:  desktop-file-utils

%description
Universal Startup Manager is a GTK4 Rust application that reads user and system
XDG autostart entries and lets you filter, toggle, add, edit, or delete
user-owned entries while keeping system entries read-only. It preserves extra
.desktop keys and localized names when rewriting files.

%prep
%autosetup -n %{name}-%{version}

%build
CARGO_NET_OFFLINE=1 cargo build --release --locked

%install
install -Dm0755 target/release/universal-startup-manager %{buildroot}%{_bindir}/universal-startup-manager
desktop-file-install --dir=%{buildroot}%{_datadir}/applications universal-startup-manager.desktop

%check
CARGO_NET_OFFLINE=1 cargo test --release --locked
desktop-file-validate universal-startup-manager.desktop

%files
%license LICENSE
%doc README.md
%{_bindir}/universal-startup-manager
%{_datadir}/applications/universal-startup-manager.desktop

%changelog
* Sun Dec 14 2025 Your Name <you@example.com> - 1.0.2-1
- Update desktop entry packaging
* Sun Dec 14 2025 Your Name <you@example.com> - 1.0.1-1
- Initial RPM release
