Name:           paper
Version:        0.1.0
Release:        1%{?dist}
Summary:        Local-first document search with exact page and line citations

License:        MIT OR Apache-2.0
URL:            https://github.com/arrowassassin/paperscope
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.88
BuildRequires:  libsqlite3-devel
Requires:       libc6 >= 2.31, libsqlite3

%description
Paper is a local-first document search CLI and MCP server that indexes your
documents and provides exact citations (file, page, line) for every search
result. It runs entirely offline with no telemetry, no accounts, and no cloud
dependencies unless explicitly opted in.

Features:
* Exact keyword and semantic search with citations
* Supports PDF, DOCX, XLSX, PPTX, EPUB, email, HTML, Markdown, and more
* Optional OCR for scanned documents
* Optional shell-out accelerators (pdftotext, pandoc, libreoffice)
* MCP server for AI agent integration
* Single static binary, no runtime dependencies

%prep
%autosetup -n paperscope-%{version}

%build
cargo build --release --locked

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}/usr/bin
install -m 755 target/release/paper %{buildroot}/usr/bin/paper

%files
%license LICENSE-MIT LICENSE-APACHE
%doc CHANGELOG.md README.md
/usr/bin/paper

%changelog
* Thu Nov 19 2026 Paper Maintainers <maintainers@paper.example> - 0.1.0-1
- Initial release