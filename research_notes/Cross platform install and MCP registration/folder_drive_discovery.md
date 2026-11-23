# Cross-platform discovery of document folders, volumes and drives

Scope: macOS 14+, mainstream Linux desktops, Windows 10 21H2+/Windows 11; no admin
rights. Code readings are from `dirs` 7.0.0 / `dirs-sys` 0.5.0, `directories`
6.0.0, `sysinfo` 0.33.1, `ignore` 0.4.33 and `walkdir` 2.5.0 unless stated
otherwise. **Paper currently pins `dirs` 5.0.1**; its `mac.rs`, `lin.rs` and
`win.rs` are behaviourally identical to 7.0.0 for every function discussed here
(hardcoded `$HOME/...` on macOS, `user-dirs.dirs` on Linux,
`SHGetKnownFolderPath` on Windows), so the 5.0.1 dependency does not change any
finding below.

---

## macOS: NSFileManager vs `dirs`, iCloud Drive, Desktop/Documents sync, TCC

### Takeaway
Use `NSFileManager.urls(for:in:)` when you need the OS's own answer (sandbox
containers, future relocations); outside the App Sandbox it returns the same
paths as `dirs`, which simply hardcodes `$HOME/Documents` etc. TCC is the real
constraint: `read_dir`/`open` on a protected folder fail with **EPERM**, `stat`
of the folder may still succeed, and there is **no public, prompt-free API** to
ask whether access has been granted. Treat protected roots as opt-in, probe
once, and surface an actionable "grant Full Disk Access" error.

### Cited Findings
- `URLsForDirectory:inDomains:` returns an array of URLs for the requested
  `NSSearchPathDirectory` in the requested domains; results are ordered
  user-domain first, then system; the call does **not** guarantee the directory
  exists. `NSSearchPathForDirectoriesInDomains` is the string-returning
  equivalent, and Apple recommends the URL variant. — [Apple Developer: URLsForDirectory:inDomains:](https://developer.apple.com/documentation/foundation/filemanager/urls(for:in:))
- Apple's File System Programming Guide says to locate standard directories via
  `URLsForDirectory:inDomains:` so paths are correct whether or not the app is
  sandboxed. — [Apple: Accessing Files and Directories](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/AccessingFilesandDirectories/AccessingFilesandDirectories.html)
- The relevant constants are `NSDocumentDirectory` (9), `NSDesktopDirectory`
  (12) and `NSDownloadsDirectory` (15); Downloads is only available when the
  domain includes the user domain. — [Microsoft Learn mirror of NSSearchPathDirectory](https://learn.microsoft.com/en-us/dotnet/api/foundation.nssearchpathdirectory?view=net-ios-26.5-10.0)
- `dirs` 7.0.0 on macOS does **not** call `NSSearchPathForDirectoriesInDomains`.
  Every folder is `home_dir().join("Documents" | "Desktop" | "Downloads" | ...)`,
  and `home_dir()` is `$HOME` (falling back to `getpwuid_r`). — [dirs 7.0.0 `src/mac.rs`](https://docs.rs/crate/dirs/7.0.0/source/src/mac.rs), [dirs-sys 0.5.0 `src/lib.rs`](https://docs.rs/crate/dirs-sys/0.5.0/source/src/lib.rs)
- `directories` 6.0.0 has the same macOS behaviour (`home_dir().join("Documents")`
  etc.); it adds a `UserDirs` struct but no OS call. — [directories 6.0.0 `src/mac.rs`](https://docs.rs/crate/directories/6.0.0/source/src/mac.rs)
- iCloud Drive's user-visible root is
  `~/Library/Mobile Documents/com~apple~CloudDocs`; app-specific iCloud folders
  live one level up under `~/Library/Mobile Documents/<team-id>~<bundle-id>`. — [Eclectic Light: How does iCloud work?](https://eclecticlight.co/2023/07/17/how-does-icloud-work-plus-a-new-version-of-cirrus), [Ask Different](https://apple.stackexchange.com/questions/428604/how-to-make-icloud-drive-conveniently-available-to-the-shell)
- When "Desktop & Documents Folders" is enabled, `~/Desktop` and `~/Documents`
  stay **real directories at their original home paths**; they are enrolled in
  the CloudDocs File Provider via the extended attribute
  `com.apple.file-provider-domain-id` (value
  `com.apple.CloudDocs.iCloudDriveFileProvider/<UUID>`). The `Documents` entry
  seen *inside* iCloud Drive is a symlink back to `~/Documents`. — [cver.net: Your Mac has two Downloads folders](https://cver.net/devlog/unify-mac-ios-downloads)
- Third-party cloud providers on modern macOS appear under
  `~/Library/CloudStorage/<Provider>-<Account>` (examples in the wild:
  `Box-Box`, `Dropbox`, `GoogleDrive-<username>`); these are File Provider
  domains, not ordinary folders. — [TheCoatlessProfessor: cloud storage symlinks on macOS](https://blog.thecoatlessprofessor.com/posts/desktop-shortcut-for-cloud-storage-macos)
- TCC protects, on current macOS: `~/Desktop`, `~/Documents`, `~/Downloads`,
  iCloud Drive, third-party cloud storage, removable volumes, network volumes
  and Time Machine backups. — [Eclectic Light: Explainer: Permissions, privacy and TCC](https://eclecticlight.co/2025/11/08/explainer-permissions-privacy-and-tcc)
- A denied protected read surfaces as **EPERM / "Operation not permitted"**, not
  EACCES / "Permission denied"; `ls ~/Documents` fails even though the directory
  mode reads `drwxr-xr-x`, because TCC sits above POSIX permissions. — [Amit Kothari: macOS TCC broke brew, mise and Claude Code](https://amitkoth.com/macos-tcc-documents-folder/)
- In practice `lstat` on a path *inside* a protected folder can return EPERM
  (`lstat '/Users/<user>/Documents/Default Project'`), while `ls` on a
  non-existent child returns ENOENT — path resolution works, content access is
  denied. Metadata operations "partially still work" in another report. — [opencode issue #56479](https://github.com/anomalyco/opencode/issues/56479), [Orca issue #13921](https://github.com/stablyai/orca/issues/13921)
- A CLI launched from Terminal inherits TCC attribution from its **responsible
  process** (Terminal/iTerm), so Terminal's grant is what counts; launched from
  Finder or `open`, the app itself is responsible. — [Qt: The Curious Case of the Responsible Process](https://www.qt.io/blog/the-curious-case-of-the-responsible-process)
- An app with no `NSDocumentsFolderUsageDescription` in `Info.plist` gets a
  **silent denial** (EPERM), not a prompt; the same binary works once Full Disk
  Access is granted and the app is fully relaunched. — [opencode issue #56479](https://github.com/anomalyco/opencode/issues/56479)
- TCC is implemented by `tccd`; frameworks call the **private** `TCCAccessRequest`
  to check/request a decision. The first request triggers a prompt; later calls
  return the stored decision. There is no public "query only" API. — [Mark Rowe: TCC and the macOS Platform Sandbox Policy](https://bdash.net.nz/posts/tcc-and-the-platform-sandbox-policy)
- Full Disk Access is formally `kTCCServiceSystemPolicyAllFiles`; it overrides
  the more granular Files & Folders grants, but cannot override POSIX
  permissions or ACLs. It must be granted manually in System Settings and the
  app restarted. — [Fileside: Full Disk Access](https://www.fileside.app/blog/2024-05-31_full-disk-access), [Eclectic Light: Privacy: Files & Folders or Full Disk Access?](https://eclecticlight.co/2026/04/08/privacy-files-folders-or-full-disk-access)
- The TCC databases live at `~/Library/Application Support/com.apple.TCC/TCC.db`
  and `/Library/Application Support/com.apple.TCC/TCC.db`; the schema is
  undocumented and changes between releases, and reading the system DB itself
  requires Full Disk Access. — [Mac Hardening: TCC permissions](https://www.machardening.com/en/blog/tcc-privacy-permissions)
- The long-standing "open `TCC.db` and check errno" Full Disk Access test is
  known to report false negatives on a small percentage of systems. — [Apple Developer Forums thread 114452](https://developer.apple.com/forums/thread/114452) (page was bot-blocked during this research; finding comes from the search result snippet)

### Inferences
- On macOS, `dirs::document_dir()` is adequate for the *default* case only. It
  cannot see iCloud Known-Folder-style enrollment, a redirected home, or an
  App Sandbox container. For a non-sandboxed CLI the hardcoded path is
  byte-identical to what `NSFileManager` returns in the normal case, so adding
  `objc2` solely for `URLsForDirectory` is not worth it yet; revisit if Paper is
  ever sandboxed or shipped as a `.app`.
- Do not add both `~/Documents` and `~/Library/Mobile Documents/com~apple~CloudDocs`
  as independent roots when Desktop & Documents sync is on: `~/Documents` is the
  real directory and iCloud Drive's `Documents` is a symlink to it. Canonicalise
  (`fs::canonicalize`) and drop equal/nested roots, or Paper will index the same
  files twice with different paths.
- Detect iCloud Desktop/Documents enrollment cheaply with
  `xattr -p com.apple.file-provider-domain-id ~/Documents` (or `getxattr(2)`),
  not by looking for a symlink.
- TCC state cannot be discovered without side effects. The only safe startup
  behaviour is: resolve the path (metadata only), do **not** enumerate it, and
  mark the candidate `requires_consent = true`. Probe only when the user selects
  it, then map `EPERM` to a specific remediation message. A probe can block on
  the consent dialog, and a headless/launchd process gets no dialog at all.
- Distinguish `EPERM` (TCC) from `EACCES` (POSIX/ACL) in error reporting; this is
  the single most useful diagnostic on macOS.
- Full Disk Access is the only practical blanket fix for a CLI that indexes
  Documents + Desktop + Downloads + external volumes; per-folder Files & Folders
  grants work but require the user to approve each location, and the grant
  belongs to the responsible terminal app.
- Removable and network volumes are TCC-protected too, so "just scan `/Volumes`"
  is not a permission-free strategy on macOS 14+.

### Gaps
- Apple's `mountedVolumeURLs(includingResourceValuesForKeys:options:)` page did
  not load during this research. The canonical volume-enumeration API and its
  resource keys (`NSURLVolumeIsLocalKey`, `NSURLVolumeIsRemovableKey`,
  `NSURLVolumeIsInternalKey`, `NSURLVolumeAvailableCapacityKey`) are therefore
  described only from prior knowledge, not cited here. `sysinfo` uses
  `getfsstat(2)` on macOS, which is a citable fallback.
- Exact behaviour of "Optimize Mac Storage" evicted files (`.icloud` placeholder
  names, whether `read_dir` lists them, how to detect without downloading) was
  not verified from a primary source in this pass.
- Whether `access(2)`/`faccessat(2)` bypasses TCC and can be used as a
  non-prompting probe is not established. Assume it cannot.
- The TCC prompt timeout (reported as ~2 minutes for Automation prompts) is not
  confirmed for Files & Folders prompts.

---

## Linux: XDG user dirs, localization, absent config

### Takeaway
Read `$XDG_CONFIG_HOME/user-dirs.dirs` (default `~/.config/user-dirs.dirs`) and
use the literal paths it contains. Never reconstruct `Documents` from an English
string and never translate it yourself: the file already contains the localized
name, and it may point outside `$HOME` (including another disk). If the file is
absent, `dirs`/`directories` return `None`; fall back to the legacy English
directories only if they exist, and mark the candidate low-confidence.

### Cited Findings
- `xdg-user-dirs-update` runs early in login, creates localized directories,
  and writes `$(XDG_CONFIG_HOME)/user-dirs.dirs`; `XDG_CONFIG_HOME` defaults to
  `~/.config`. — [freedesktop.org: xdg-user-dirs](https://www.freedesktop.org/wiki/Software/xdg-user-dirs)
- The file contains lines `XDG_<NAME>_DIR=<VALUE>` for `DESKTOP`, `DOWNLOAD`,
  `TEMPLATES`, `PUBLICSHARE`, `DOCUMENTS`, `MUSIC`, `PICTURES`, `VIDEOS`;
  `VALUE` must be `"$HOME/Path"` or `"/Path"`; `#` starts a comment; the format
  is designed to be sourced by a shell. — [user-dirs.dirs(5), Arch manual](https://man.archlinux.org/man/user-dirs.dirs.5.en)
- "To disable a directory, point it to the homedir"; if the file is deleted it
  is recreated on next login, and configured directories that no longer exist
  are reset to the home directory. — [freedesktop.org: xdg-user-dirs](https://www.freedesktop.org/wiki/Software/xdg-user-dirs), [xdg-user-dirs-update(1)](https://manpages.debian.org/unstable/xdg-user-dirs/xdg-user-dirs-update.1.en.html)
- Example fallback logic from the spec itself: source the file if it exists, then
  `echo ${XDG_DESKTOP_DIR:-$HOME/Desktop}` and
  `echo ${XDG_DOWNLOAD_DIR:-$HOME}`. — [freedesktop.org: xdg-user-dirs](https://www.freedesktop.org/wiki/Software/xdg-user-dirs)
- Localized directory names are the norm (e.g. `~/Dokumente`, `~/Documenti`,
  `~/ドキュメント`); the tool's whole purpose is translation of the filenames.
  — [freedesktop.org: xdg-user-dirs](https://www.freedesktop.org/wiki/Software/xdg-user-dirs), [ArchWiki: XDG user directories](https://wiki.archlinux.org/title/XDG_user_directories)
- `dirs` 7.0.0 on Linux parses the file via `dirs-sys::user_dir("DOCUMENTS")`
  etc.; it does **not** fall back to `$HOME/Documents`. The parser only accepts
  double-quoted values, handles `$HOME/...` and absolute paths, applies POSIX
  double-quote unescaping, and skips a value equal to `$HOME/` (the "disabled"
  convention). — [dirs 7.0.0 `src/lin.rs`](https://docs.rs/crate/dirs/7.0.0/source/src/lin.rs), [dirs-sys 0.5.0 `src/xdg_user_dirs.rs`](https://docs.rs/crate/dirs-sys/0.5.0/source/src/xdg_user_dirs.rs)
- `directories` 6.0.0 on Linux uses the same `dirs-sys` parser and exposes the
  results as `Option<PathBuf>` fields on `UserDirs`. — [directories 6.0.0 `src/lin.rs`](https://docs.rs/crate/directories/6.0.0/source/src/lin.rs)
- `xdg-user-dir NAME` looks up one value in `user-dirs.dirs`; it is a debugging
  aid, not something a library needs to shell out to. — [xdg-user-dir(1)](https://man.archlinux.org/man/extra/xdg-user-dirs/xdg-user-dir.1.en)
- `xdg-user` 0.3.0 parses the same file; `UserDirs::new()` parses once and the
  root functions parse on every call; each getter returns
  `Result<PathBuf, Error>` and only succeeds "if found". It also exposes
  `XDG_PROJECTS_DIR`, which is not in the base spec. — [xdg-user 0.3.0 docs](https://docs.rs/xdg-user/latest/xdg_user/)

### Inferences
- Candidate resolution order for Linux should be:
  1. explicit CLI/config root;
  2. `$XDG_DOCUMENTS_DIR`, `$XDG_DESKTOP_DIR`, `$XDG_DOWNLOAD_DIR` **if set,
     absolute and existing** (home-manager and some distros export these);
  3. the same keys parsed from `user-dirs.dirs`;
  4. legacy `~/Documents`, `~/Desktop`, `~/Downloads` only if they exist;
  5. other XDG dirs (`Pictures`, `Music`, `Videos`, `Public`, `Projects`) with
     lower rank;
  6. user home as a low-ranked, opt-in root.
- A `user-dirs.dirs` value of `"$HOME"` means "disabled", not "index the whole
  home directory"; `dirs` correctly skips it.
- Because values may be absolute paths outside `$HOME`, an XDG Documents folder
  can live on a second disk. Treat it as a normal candidate and run the mount
  checks from the next section on its filesystem, not just on `$HOME`.
- `dirs`' parser is intentionally narrow (double quotes only). Real
  `xdg-user-dirs-update` output is double-quoted, so this is fine; hand-edited
  single-quoted files are out of spec.
- `xdg-user` gives better "missing vs unreadable" errors than `dirs`, but Paper
  already depends on `dirs`; the extra crate is only justified if you want
  `XDG_PROJECTS_DIR` or a single parse of all dirs.

### Gaps
- No reliable source found for which desktop environments/localized installs
  omit `user-dirs.dirs` entirely, or how often `XDG_*_DIR` is exported into the
  session environment. The safe assumption is "either may be missing".
- Snap and Flatpak confinement changes what `$HOME` and `/media` look like; see
  the mount section for the confinement caveats, which are not fully citable
  from the sources used here.

---

## Linux: enumerating and filtering mounts (`/proc/self/mountinfo`, `findmnt`)

### Takeaway
Parse `/proc/self/mountinfo` (not `/proc/mounts`) when you need to distinguish
bind mounts, btrfs subvolumes and duplicate mount points; it exposes mount ID,
parent ID, `major:minor`, the filesystem root, mount point, per-mount options,
optional fields and the filesystem type. Use `sysinfo::Disks` if you want a
maintained cross-platform wrapper, but understand its filters: it already drops
most pseudo-filesystems, `squashfs`, `iso9660`, and (by default) `tmpfs` and
NFS/CIFS, and it can optionally include them via features.

### Cited Findings
- `/proc/pid/mountinfo` exists since Linux 2.6.26 and supplies information
  missing from `/proc/pid/mounts`: propagation state, root of the mount for bind
  mounts, and an identifier for each mount and its parent. Line format:
  `36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw,errors=continue`
  with fields (1) mount ID, (2) parent ID, (3) `major:minor` = `st_dev`,
  (4) root, (5) mount point, (6) per-mount options, (7) optional fields,
  (8) `-` separator, (9) filesystem type `type[.subtype]`, (10) source,
  (11) superblock options. — [proc_pid_mountinfo(5)](https://man7.org/linux/man-pages/man5/proc_pid_mountinfo.5.html)
- `sysinfo` 0.33.1 on Linux reads `/proc/mounts` and parses
  `fs_spec`, `fs_file`, `fs_vfstype`. It filters out `rootfs`, `sysfs`, `proc`,
  `devtmpfs`, `cgroup`, `cgroup2`, `pstore`, `squashfs`, `rpc_pipefs`,
  `iso9660`, `devpts`, `hugetlbfs`, `mqueue`; `tmpfs` is included only with the
  `linux-tmpfs` feature and `cifs`/`nfs`/`nfs4` only with `linux-netdevs`. It
  also drops mount points starting with `/sys`, `/proc`, or `/run` (except
  `/run/media`) and sources starting with `sunrpc`. — [sysinfo 0.33.1 `src/unix/linux/disk.rs`](https://docs.rs/crate/sysinfo/0.33.1/source/src/unix/linux/disk.rs)
- `sysinfo` marks a disk removable by matching its device against
  `/dev/disk/by-id/usb-*` entries, and computes `is_read_only` from
  `statvfs`'s `ST_RDONLY`. — [sysinfo 0.33.1 `src/unix/linux/disk.rs`](https://docs.rs/crate/sysinfo/0.33.1/source/src/unix/linux/disk.rs)
- `findmnt` can emit machine-readable output (`--json`) and filter to real
  filesystems (`--real`), which makes it a useful fallback for debugging:
  `findmnt --json --real --output TARGET,SOURCE,FSTYPE,OPTIONS`. (util-linux
  man page; not fetched in this pass.)

### Inferences
- Recommended filter for Paper's default crawl:
  - **Allow** real local filesystems by type: `ext4`, `ext3`, `ext2`, `xfs`,
    `btrfs`, `f2fs`, `zfs`, `vfat`, `exfat`, `ntfs3`, `fuseblk` (NTFS-3G),
    `hfsplus`, `apfs` (if a fuse driver is present).
  - **Deny** pseudo/volatile: `proc`, `sysfs`, `devtmpfs`, `devpts`, `tmpfs`,
    `cgroup`, `cgroup2`, `pstore`, `securityfs`, `debugfs`, `tracefs`,
    `configfs`, `fusectl`, `mqueue`, `hugetlbfs`, `bpf`, `autofs`, `ramfs`,
    `nsfs`, `overlay` (containers), `squashfs`/`erofs` (snaps, read-only images).
  - **Deny by default, opt-in** network: `nfs`, `nfs4`, `cifs`, `smb3`, `sshfs`,
    `9p`, `ceph`, `glusterfs`, `fuse.rclone`, `fuse.sshfs`, `davfs`.
  - **Deny by default, opt-in** removable: anything whose backing device is
    marked removable in `/sys` or whose mount point is under `/media`,
    `/run/media`, `/mnt` with a removable device.
  - **Deny** read-only mounts (`ro` in mount options / `ST_RDONLY`) for indexing
    writes, but they can still be read-only candidates if the user asks.
- For duplicate/overlapping mounts:
  - Deduplicate by `(major:minor, root)` first; that catches bind mounts of the
    same superblock and multiple mount points for the same filesystem.
  - Then canonicalise mount points and drop any mount point that is equal to or
    nested under another accepted mount point, unless the user explicitly added
    it.
  - Do not rely on `st_dev` alone to stop a walk on btrfs: subvolume mounts can
    share a device while having different `root` values in mountinfo. Use the
    `root` field and canonical paths; `walkdir`/`ignore` `same_file_system`
    compares `st_dev` and is therefore a useful but not complete guard.
- Snap: the snap runtime mounts `squashfs` images read-only under
  `/snap/<name>/<rev>`; those are excluded by the `squashfs` rule. A confined
  snap sees a private `/` and usually cannot read `/media` or arbitrary home
  paths; expect `EACCES`/missing paths rather than a useful mount list.
- Flatpak: a sandboxed app sees only the paths granted by
  `--filesystem=...`; documents are often exposed through the document portal
  as FUSE mounts under `/run/user/<uid>/doc/<id>`. Those are not stable
  user-facing paths and should not be indexed as if they were the user's
  Documents folder. A native CLI installed via cargo/Homebrew is unconfined and
  does not have this problem.
- Home on NFS: if `$HOME` is an NFS/autofs mount, default network exclusion would
  exclude the user's own home. Detect this case explicitly and either (a) allow
  the home mount even though it is network, with a warning, or (b) refuse and
  ask for a local root.
- `sysinfo` is a reasonable first pass because it is already a dependency, but
  its Linux source is `/proc/mounts` and it does not expose the `root` field or
  per-mount options, so it cannot do precise bind/subvolume dedupe. A ~100-line
  mountinfo parser is the more correct foundation; `sysinfo` remains useful for
  capacity/free-space and for Windows/macOS volume enumeration.

### Gaps
- Exact `st_dev` behaviour across btrfs subvolumes is not confirmed here. The
  safe design is not to depend on it.
- How reliably `/sys/class/block/<dev>/removable` identifies USB vs internal
  NVMe on all desktops was not verified; `sysinfo` uses `/dev/disk/by-id/usb-*`
  instead, which is a different heuristic.
- Flatpak portal path stability and whether the portal FUSE mount is visible to
  a non-sandboxed process were not verified from a primary source.

---

## Windows: `SHGetKnownFolderPath`, Known Folder Move, drives and volumes

### Takeaway
Never build `%USERPROFILE%\Documents`. Call `SHGetKnownFolderPath` with
`FOLDERID_Documents`, `FOLDERID_Desktop` and `FOLDERID_Downloads`; it returns
the **current, redirected** location, including OneDrive Known Folder Move.
`dirs` already does exactly this on Windows, so the fix for Paper is to stop
guessing `$HOME/Documents` and use `dirs::document_dir()` (or `known-folders`
for the extra IDs). Enumerate volumes with `FindFirstVolumeW` +
`GetVolumePathNamesForVolumeNameW` + `GetDriveTypeW`, not just
`GetLogicalDrives`, because a volume can be mounted only in a folder.

### Cited Findings
- For new code, always use `SHGetKnownFolderPath` with `KNOWNFOLDERID`
  constants; `SHGetFolderPath`/CSIDL are deprecated. COM must be initialized on
  the calling thread (`CoInitializeEx`), and the returned string must be freed
  with `CoTaskMemFree`. — [Microsoft Learn: Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/known-folders)
- `SHGetKnownFolderPath(rfid, dwFlags, hToken, &ppszPath)` returns the path for
  the current user when `hToken` is `NULL`; the path has no trailing backslash.
  `E_FAIL` can mean the KNOWNFOLDERID has no path (virtual folder);
  `E_INVALIDARG` can mean the folder is not present on this system. — [Microsoft Learn: SHGetKnownFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath)
- `FOLDERID_Documents` is `{FDD39AD0-238F-46AF-ADB4-6C85480369C7}`;
  `FOLDERID_LocalDocuments` is `{f42ee2d3-909f-4907-8871-4c22fc0bf756}`.
  `FOLDERID_Downloads` is `{374DE290-123F-4565-9164-39C4925E467B}`;
  `FOLDERID_SkyDrive` (OneDrive) is `{A52BBA46-E9E1-435f-B3D9-28DAA648C0F6}`,
  default `%USERPROFILE%\OneDrive`, introduced in Windows 8.1. — [Microsoft Learn: KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)
- Microsoft explicitly says: to redirect Documents, Pictures or Desktop to
  OneDrive, use OneDrive Known Folder Move, not the older redirection method.
  — [Microsoft Learn: Working with Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders)
- Known Folder Move is controlled by OneDrive policies (Group Policy, Intune, or
  registry); the policy set covers prompting users to move Windows known folders
  to OneDrive. — [Microsoft Learn: Redirect and move Windows known folders to OneDrive](https://learn.microsoft.com/en-us/onedrive/redirect-known-folders)
- A community Q&A on Windows 11 + KFM reports that folder locations are tracked
  in the registry once OneDrive backup is enabled, and that
  `SHGetKnownFolderPath(FOLDERID_Documents, KF_FLAG_DEFAULT, ...)` returns the
  redirected path while `KF_FLAG_DEFAULT_PATH` returns the default path. A
  Microsoft staff participant explicitly could **not** verify this because they
  had no OneDrive test environment. — [Microsoft Q&A: known folder paths with OneDrive KFM](https://learn.microsoft.com/en-us/answers/questions/2085166/what-are-the-known-folder-paths-returned-by-shgetk)
- `dirs` 7.0.0 on Windows maps `document_dir()` → `known_folder_documents()` →
  `SHGetKnownFolderPath(FOLDERID_Documents)`, and `home_dir()` →
  `FOLDERID_Profile`; it does **not** use `%USERPROFILE%` or
  `%HOMEDRIVE%%HOMEPATH%`. — [dirs 7.0.0 `src/win.rs`](https://docs.rs/crate/dirs/7.0.0/source/src/win.rs), [dirs-sys 0.5.0 `src/lib.rs`](https://docs.rs/crate/dirs-sys/0.5.0/source/src/lib.rs)
- `GetLogicalDrives()` returns a bitmask of drive letters currently assigned to
  an existing logical drive (local volume, removable media, or mapped network
  share); cleared bits are unassigned letters. — [Microsoft Learn: GetLogicalDrives](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getlogicaldrives)
- `GetDriveTypeW` returns `DRIVE_UNKNOWN` (0), `DRIVE_NO_ROOT_DIR` (1),
  `DRIVE_REMOVABLE` (2), `DRIVE_FIXED` (3), `DRIVE_REMOTE` (4), `DRIVE_CDROM`
  (5), `DRIVE_RAMDISK` (6); the root path needs a trailing backslash. It does
  not distinguish USB from internal; for that, query
  `SetupDiGetDeviceRegistryProperty` with `SPDRP_REMOVAL_POLICY`. — [Microsoft Learn: GetDriveTypeW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdrivetypew)
- Mounted folders (volumes mounted at a directory rather than a drive letter)
  are enumerated with `FindFirstVolumeMountPoint` / `FindNextVolumeMountPoint` /
  `FindVolumeMountPointClose`; NTFS is required for mounted folders. — [Microsoft Learn: Enumerating Mounted Folders](https://learn.microsoft.com/en-us/windows/win32/fileio/enumerating-volume-mount-points)
- To test whether a directory is a mounted folder: call `GetFileAttributes` and
  check `FILE_ATTRIBUTE_REPARSE_POINT`, then read the reparse tag from
  `WIN32_FIND_DATA.dwReserved0`; `IO_REPARSE_TAG_MOUNT_POINT` means a volume
  mount point, `IO_REPARSE_TAG_SYMLINK` means a symlink. — [Microsoft Learn: Determining Whether a Directory Is a Mounted Folder](https://learn.microsoft.com/en-us/windows/win32/fileio/determining-whether-a-directory-is-a-volume-mount-point)
- `FindFirstVolumeW` / `FindNextVolumeW` return volume GUID paths
  (`\\?\Volume{GUID}\`); `GetVolumePathNamesForVolumeNameW` maps a volume to all
  of its drive letters and folder mount points. `sysinfo` uses exactly this on
  Windows, filters `GetDriveTypeW` to `DRIVE_FIXED`/`DRIVE_REMOVABLE`, skips
  volumes with no mount paths, and reads size with `GetDiskFreeSpaceExW`. —
  [sysinfo 0.33.1 `src/windows/disk.rs`](https://docs.rs/crate/sysinfo/0.33.1/source/src/windows/disk.rs)

### Inferences
- The correct Windows candidate order is:
  1. `FOLDERID_Documents` (current, redirected);
  2. `FOLDERID_Desktop`;
  3. `FOLDERID_Downloads`;
  4. `FOLDERID_OneDrive` (if present) — but only as a *container* candidate;
     it will overlap Documents/Desktop under KFM;
  5. `FOLDERID_PublicDocuments`, `FOLDERID_Pictures`, etc. (lower rank);
  6. fixed-volume roots (very low rank; usually not what the user means);
  7. removable/network volumes (opt-in).
- KFM detection heuristics, in order of confidence:
  1. Compare `FOLDERID_Documents` with `FOLDERID_LocalDocuments`. If they
     differ, Documents has been redirected (OneDrive or another redirector).
     This is the cleanest signal, but it rests on the community-reported
     `LocalDocuments` behaviour.
  2. Check whether the resolved `FOLDERID_Documents` path is equal to or under
     `FOLDERID_OneDrive`. This is robust when OneDrive is installed and the
     folder is genuinely inside it.
  3. Check the `OneDrive`, `OneDriveConsumer`, `OneDriveCommercial` environment
     variables and the `HKCU\Software\Microsoft\OneDrive` tree for the account
     folder. Treat these as hints, not proof.
  4. Fall back to `%USERPROFILE%\OneDrive` only if it exists.
- Drop the assumption that `%USERPROFILE%\Documents` still contains the user's
  documents. Under KFM it is typically empty or stale; indexing it produces
  zero hits and a confusing "no documents" result.
- KFM covers Desktop, Documents and Pictures, not Downloads, per Microsoft's
  "Documents, Pictures, or Desktop" wording. Downloads normally stays at
  `%USERPROFILE%\Downloads` even when KFM is on. — inference from
  [Working with Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders)
- `GetLogicalDrives` is insufficient for a document indexer: a data disk can be
  mounted at `C:\Data` with no letter, and `GetLogicalDrives` will not show it.
  Use the volume enumeration path above, then `GetVolumePathNamesForVolumeNameW`
  to get every mount point.
- Filter volumes as follows:
  - `DRIVE_FIXED`: include as a low-ranked candidate, but do not crawl the whole
    volume by default.
  - `DRIVE_REMOVABLE`: opt-in only; slow, may disappear mid-scan, may be
    BitLocker-locked.
  - `DRIVE_REMOTE`: exclude by default (network, latency, credentials).
  - `DRIVE_CDROM`, `DRIVE_RAMDISK`, `DRIVE_NO_ROOT_DIR`, `DRIVE_UNKNOWN`:
    exclude.
  - Skip volume GUID paths that have no `GetVolumePathNamesForVolumeNameW`
    entries (recovery/EFI partitions, unmounted volumes).
- `%USERPROFILE%` vs `%HOMEDRIVE%%HOMEPATH%`: use `FOLDERID_Profile` (what `dirs`
  does). `HOMEDRIVE`/`HOMEPATH` are logon-environment variables that can differ
  under `runas`, scheduled tasks or redirected profiles, and concatenating them
  is a classic source of wrong paths. `USERPROFILE` is closer but still a
  string; the known-folder API is authoritative.
- Virtual drives: Google Drive for desktop, Dropbox and similar products can
  expose a drive letter backed by a virtual filesystem. `GetDriveTypeW` may
  report `DRIVE_FIXED` or `DRIVE_REMOTE` depending on the product/version, so
  drive type alone cannot classify "cloud". Detect them by volume label
  (`GetVolumeInformationW`), by known install paths, and by the
  `RECALL_ON_DATA_ACCESS` attribute on their files (next section).
- BitLocker: a locked volume can still have a drive letter and report
  `DRIVE_FIXED`; opening its root fails (access denied / not ready) until the
  user unlocks it. Do not attempt to unlock; report the volume as locked and
  skip it.

### Gaps
- Microsoft staff could not confirm the exact `SHGetKnownFolderPath`/
  `FOLDERID_LocalDocuments` behaviour under OneDrive KFM. The Q&A is the best
  source found, but it is community-reported, not a Microsoft guarantee.
- The exact registry values under
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\User Shell Folders`
  (`Personal`, `Desktop`, `{374DE290-...}` for Downloads) and the
  `Shell Folders` expanded cache were not verified from a primary Microsoft
  page in this pass. Prefer the API; use the registry only as a diagnostic.
- BitLocker error codes (`ERROR_ACCESS_DENIED` vs `ERROR_NOT_READY`) and
  whether `GetDriveTypeW` can block on a locked volume were not verified.
- Whether `GetDriveTypeW` returns `DRIVE_FIXED` or `DRIVE_REMOTE` for current
  Google Drive/Dropbox virtual drives is product- and version-dependent and was
  not verified.

---

## Windows: reparse points, cloud placeholders, and avoiding hydration/double-indexing

### Takeaway
Enumerate metadata only, inspect attributes **before** opening anything, and
skip cloud-only entries by default. `FindFirstFileExW` with
`FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY` restricts results to files physically on
disk; `GetFileAttributesExW` is metadata-only; opening a placeholder for reading
is what hydrates it. Treat junctions/mount points as traversal boundaries, and
deduplicate OneDrive/cloud roots against the OS known folders.

### Cited Findings
- `FILE_ATTRIBUTE_REPARSE_POINT` (0x400) marks a file/directory with a reparse
  point or symbolic link. `FILE_ATTRIBUTE_OFFLINE` (0x1000) means the data is
  not available immediately. `FILE_ATTRIBUTE_RECALL_ON_OPEN` (0x40000) appears
  only in directory-enumeration classes: the item is virtual and opening it will
  fetch at least some data. `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` (0x400000)
  means the file/directory is not fully present locally; reading/enumerating it
  will fetch data from a remote store. `FILE_ATTRIBUTE_PINNED` (0x80000) and
  `FILE_ATTRIBUTE_UNPINNED` (0x100000) are HSM hints. — [Microsoft Learn: File Attribute Constants](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)
- `FindFirstFileExW(..., dwAdditionalFlags)` supports
  `FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY` (4): "Limits the results to files that
  are physically on disk. This flag is only relevant when a file virtualization
  filter is present." — [Microsoft Learn: FindFirstFileExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)
- If the path points to a symbolic link, `WIN32_FIND_DATA` describes the link,
  not the target. — [Microsoft Learn: FindFirstFileExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)
- Reparse points are a tag + up to 16 KB of data; NTFS uses them for symlinks,
  mount points and HSM stubs; a file system filter associated with the tag
  interprets the data. — [Microsoft Learn: Reparse Points](https://learn.microsoft.com/en-us/windows/win32/fileio/reparse-points)
- Mounted-folder detection is `GetFileAttributes` →
  `FILE_ATTRIBUTE_REPARSE_POINT` → read `dwReserved0` from
  `FindFirstFile`/`FindNextFile` → compare with `IO_REPARSE_TAG_MOUNT_POINT`
  (or `IO_REPARSE_TAG_SYMLINK`). — [Microsoft Learn: Determining Whether a Directory Is a Mounted Folder](https://learn.microsoft.com/en-us/windows/win32/fileio/determining-whether-a-directory-is-a-volume-mount-point)
- `known-folders` 1.4.2 is a Windows-only safe wrapper around
  `SHGetKnownFolderPath`; it exposes 142 `KnownFolder` variants including
  `Documents`, `Desktop`, `Downloads`, `LocalDocuments`, `LocalDownloads`,
  `OneDrive`, `SkyDrive`, `PublicDocuments`, `ProgramFiles`, and returns
  `Option<PathBuf>`. The crate is empty on non-Windows. — [known-folders 1.4.2](https://docs.rs/known-folders/latest/known_folders/)

### Inferences
- Safe enumeration recipe for each directory:
  1. `FindFirstFileExW(dir\*, FindExInfoBasic, ..., FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY)`
     to get attributes without hydrating.
  2. If `dwFileAttributes & (RECALL_ON_DATA_ACCESS | RECALL_ON_OPEN | OFFLINE)`
     is non-zero, classify as `cloud_only`/`stub` and skip by default.
  3. If `dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT`:
     - tag `IO_REPARSE_TAG_MOUNT_POINT` → volume mount point; do not descend
       (it is a different volume, already enumerated separately);
     - tag `IO_REPARSE_TAG_SYMLINK` → do not descend unless explicitly allowed;
     - OneDrive/cloud tags (e.g. `IO_REPARSE_TAG_CLOUD_*`) → it is a
       placeholder; do not open.
  4. Only then open/parse the file. `GetFileAttributesExW` is the metadata-only
     fallback if you are not already using `FindFirstFileExW`.
- Do **not** use `std::fs::read_dir` alone if you need attribute-aware
  skipping: it gives you `DirEntry` + `file_type`, but on Windows the cloud
  recall bits are not surfaced in a portable way. You will need a small
  `windows-sys`/`windows` helper for `WIN32_FIND_DATA.dwFileAttributes`, or a
  crate that exposes them.
- `std::fs::FileType::is_symlink()` on Windows treats name-surrogate reparse
  points (symlinks and mount points) as symlinks, so `walkdir` with
  `follow_links(false)` will yield a junction but not descend into it. That is
  the correct default. Cloud placeholders are not name surrogates, so they are
  yielded as ordinary files — hence the attribute check above is still needed.
- To avoid hydrating a placeholder even when you only want a hash: hashing reads
  the whole file and will hydrate it. For Paper, either skip cloud-only files or
  require an explicit `--include-cloud` flag and warn that it will download
  data.
- Double-indexing scenarios:
  - `FOLDERID_Documents` == `C:\Users\u\OneDrive\Documents` and
    `FOLDERID_OneDrive` == `C:\Users\u\OneDrive`. Adding both roots indexes
    Documents twice (once as a subtree of OneDrive, once directly). Drop nested
    roots after canonicalisation, or index only the highest-ranked non-overlapping
    set.
  - `C:\Users\u\Documents` (stale local folder) and the KFM path both exist.
    Only the KFM path is current; the stale one is usually empty, but can contain
    old files. Prefer `FOLDERID_Documents`; list the stale path only as a
    low-ranked "legacy" candidate if it exists and is non-empty.
  - A volume mounted both at `D:\` and at `C:\Data` appears twice. Deduplicate
    by volume GUID (`GetVolumeNameForVolumeMountPoint`) and index only one mount
    point per volume.
  - A file reachable through a junction and its target. Canonicalise with
    `GetFinalPathNameByHandle` (or `fs::canonicalize`, which returns a verbatim
    path) before storing the path; alternatively store a stable file identity
    (volume serial + file index from `GetFileInformationByHandle`) as the
    dedupe key.
- For the index key, prefer a stable file ID over the display path. Paths change
  with drive letters, junctions and OneDrive redirection; volume serial + file
  index survives renames within a volume and lets you dedupe hard links and
  multiple mount points.

### Gaps
- The exact OneDrive reparse tag values (`IO_REPARSE_TAG_CLOUD`,
  `IO_REPARSE_TAG_CLOUD_1` … `_9`) were not fetched from Microsoft docs in this
  pass; only the generic reparse-point and attribute docs were verified.
- Whether `FindFirstFileExW` with `FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY` also
  excludes directories that are partially virtualized, or only files, was not
  verified. The doc says "files that are physically on disk"; test on a real
  OneDrive Files On-Demand machine.
- Behaviour of `GetFileAttributesExW` on a cloud-only placeholder (does it
  return `RECALL_ON_DATA_ACCESS` without hydrating?) is expected but not
  explicitly documented on the pages fetched.

---

## Safe walking: cycles, exclusions, permission denial, budgets, multi-TB drives

### Takeaway
Default to **not following** symlinks/junctions, **not crossing** filesystem
boundaries, and **not** crawling whole volumes. Use a mature walker
(`walkdir` or `ignore`) with an explicit exclusion predicate, treat per-entry
errors as non-fatal, and enforce depth/entry/time/size budgets. On multi-TB
machines, index known folders and user-selected roots incrementally; never
enumerate every file on every volume at startup.

### Cited Findings
- `walkdir` does not follow symbolic links by default; when `follow_links(true)`
  is enabled, broken links or loops yield an error. — [walkdir 2.5.0 docs](https://docs.rs/walkdir/latest/walkdir/)
- `walkdir::WalkDir::same_file_system(true)` prevents descending into
  directories on a different filesystem from the root; it is supported on Unix
  and Windows, and on unsupported platforms the traversal immediately returns an
  error and yields nothing. — [walkdir 2.5.0 `src/lib.rs`](https://docs.rs/crate/walkdir/2.5.0/source/src/lib.rs)
- `walkdir` keeps an ancestor stack (with open file handles on Windows) solely
  to detect loops when following links. — [walkdir 2.5.0 `src/lib.rs`](https://docs.rs/crate/walkdir/2.5.0/source/src/lib.rs)
- `ignore`'s `WalkBuilder` defaults to `follow_links: false` and
  `same_file_system: false`; when following links, loop detection is performed.
  — [ignore 0.4.33 `src/walk.rs`](https://docs.rs/crate/ignore/0.4.33/source/src/walk.rs)
- `ignore`'s `WalkBuilder::same_file_system(true)` has the same semantics and
  platform caveat as `walkdir`. — [ignore 0.4.33 `src/walk.rs`](https://docs.rs/crate/ignore/0.4.33/source/src/walk.rs)
- `ignore`'s default ignore matcher enables `hidden`, `ignore`, `parents`,
  `git_global`, `git_ignore`, `git_exclude` and `require_git` — i.e. by default
  it skips hidden entries and honours `.gitignore`/`.ignore` files. — [ignore 0.4.33 `src/dir.rs`](https://docs.rs/crate/ignore/0.4.33/source/src/dir.rs)
- `walkdir` has `max_depth`, `max_open`, `filter_entry` and per-entry `Result`;
  a permission error on one entry is yielded as an error and traversal
  continues with siblings (standard iterator behaviour). — [walkdir 2.5.0 docs](https://docs.rs/walkdir/latest/walkdir/)

### Inferences
- Recommended walker for Paper: keep `walkdir` (already a dependency) for the
  single-threaded, predictable path; use `ignore` only if you want parallelism
  and are willing to explicitly disable its git/hidden filters. `jwalk` is
  faster for very large trees but is unmaintained (see crate section).
- Exclusion predicate, applied to the **file name** at every depth (case-
  insensitive on Windows/macOS, case-sensitive on Linux where appropriate):
  - Windows: `Windows`, `Program Files`, `Program Files (x86)`, `ProgramData`,
    `$Recycle.Bin`, `System Volume Information`, `Recovery`, `PerfLogs`,
    `AppData`, `node_modules`, `.git`, `target`, `$WinREAgent`, `Config.Msi`.
  - macOS: `.Trash`, `Library/Caches`, `Library/Application Support/Caches`,
    `node_modules`, `.git`, `target`, `.build`, `DerivedData`, and Paper's own
    data directory (`PAPER_HOME` / `~/Library/Application Support/paper`).
  - Linux: `.cache`, `.local/share/Trash`, `node_modules`, `.git`, `target`,
    `.venv`, `venv`, `__pycache__`, `.gradle`, `.m2`, `snap`, `flatpak`, and
    Paper's own `$XDG_DATA_HOME/paper`.
- Exclusion must be by **path component**, not substring: `/home/u/Documents`
  must not be excluded because it contains "Documents" in some deny list; and
  `C:\Users\u\Documents\Windows` should be excluded because the component is
  literally `Windows`, not because the full path contains it.
- Do not exclude all of `Library` on macOS: user documents can live under
  `~/Library/Mobile Documents` (iCloud) and `~/Library/CloudStorage`. Exclude
  known cache subpaths instead, or include iCloud/CloudStorage explicitly and
  prune the rest.
- Permission handling:
  - `read_dir` error on a directory: record `(path, errno)`, increment a
    per-root denied counter, continue with siblings. Do not abort the root.
  - If the **root** itself is unreadable, mark the candidate `unreadable` and
    report a remediation hint. On macOS, `EPERM` → TCC/FDA; `EACCES` → POSIX
    permissions/ACL. On Windows, `ERROR_ACCESS_DENIED` (5) → ACL or
    BitLocker/Controlled Folder Access; `ERROR_NOT_READY` (21) → volume not
    ready/locked; `ERROR_PATH_NOT_FOUND` (3) → stale known folder.
  - Do not retry permission failures in a loop. Cache the failure per root for
    the duration of the run.
- Budgets (defaults, tune per platform):
  - `max_depth`: 12–16 for known folders; 3–4 for volume roots.
  - `max_entries_per_root`: 250k–1M; stop and report "budget exhausted" rather
    than silently truncating.
  - `max_file_size`: 50–100 MB (Paper's extractors will choke on multi-GB
    videos/archives anyway).
  - `max_seconds_per_root`: 60–300 s at startup; continue incrementally in the
    background.
  - `max_open`: 64–128 on Windows, 256 on Unix; `walkdir` closes old handles
    when the fd limit is reached.
- Multi-TB drives:
  - Never full-scan a volume root by default. Rank known folders first; make
    volume roots opt-in.
  - Persist per-root crawl state (last complete mtime, directory cursor,
    seen inode/file-id set) and resume; an interrupted multi-hour crawl must not
    restart from zero.
  - Use `mtime + size` as the cheap change signal; hash only when content
    changed. For filesystems with coarse timestamps (FAT/exFAT 2 s), add a
    size check and optionally a quick hash.
  - Consider one worker thread per **physical** device, not per root: parallel
    reads on the same HDD thrash; NVMe can take more. `jwalk`/`ignore`
    parallelise per directory, which is good for many small files and bad for
    a single directory with millions of entries.
  - Avoid `canonicalize` per file; canonicalise roots once and use
    `openat`-style relative traversal or `walkdir`'s paths. On Windows,
    `GetFinalPathNameByHandle` is a per-file syscall; use it only for roots and
    for suspected junctions.
  - Watch for filesystem recursion via bind mounts/junctions even when not
    following symlinks: a bind mount appears as a normal directory. Compare
    `st_dev` (Unix) or volume serial (Windows) at each directory against the
    root, and prune on change; this is what `same_file_system` does.
- Filesystem-specific traps:
  - macOS: external volumes are often mounted `noowners`; POSIX ownership is
    ignored, but TCC still applies. SMB/AFP network mounts are TCC-protected and
    slow.
  - Linux: `autofs` mount points can trigger a mount on `stat`/`read_dir`;
    excluding `autofs` from the fstype allowlist avoids surprise network mounts.
  - Windows: `System Volume Information` and `$Recycle.Bin` are ACL-protected;
    `GetFileAttributes` may succeed but `read_dir` fails. Exclude by name early
    to avoid noisy errors.
  - Windows: paths > `MAX_PATH` need `\\?\` (or long-path opt-in); `std::fs`
    generally handles verbatim paths, but third-party walkers may not. Test with
    a deep OneDrive path.

### Gaps
- No source found quantifying the cost/behaviour of `read_dir` on multi-million-
  entry directories on APFS/NTFS/ext4; the budgets above are engineering
  judgement, not measured.
- The exact `walkdir` error kind/message for a symlink loop was not captured
  (docs say "an error is yielded"; the implementation uses an ancestor stack).
- Windows Controlled Folder Access can deny writes/reads independently of ACLs;
  its interaction with a document indexer was not researched.

---

## Crate comparison: `dirs`, `directories`, `known-folders`, `sysinfo`, `windows`, `fs_extra`, `jwalk`, `ignore`, `walkdir`, `xdg-user`

### Takeaway
For **folder resolution**, `dirs` is sufficient on Windows and Linux, and
acceptable on macOS outside a sandbox; `known-folders` is the better Windows
tool when you need `FOLDERID_OneDrive`/`FOLDERID_LocalDocuments`; `xdg-user`
adds better Linux errors but no new capability. For **volume enumeration**,
`sysinfo::Disks` is a good cross-platform first pass, with the caveat that its
Linux source is `/proc/mounts` and it cannot do precise bind/subvolume dedupe.
For **walking**, `walkdir` is the safe, mature default; `ignore` is the parallel
upgrade but must have its git/hidden filters disabled; `jwalk` is fast but
unmaintained; `fs_extra` is irrelevant to discovery.

### Cited Findings
- `dirs` 7.0.0:
  - macOS: hardcodes `$HOME/Documents`, `$HOME/Desktop`, `$HOME/Downloads`,
    `$HOME/Movies`, `$HOME/Pictures`, `$HOME/Public`, `$HOME/Music`; no
    `NSSearchPathForDirectoriesInDomains`. — [dirs 7.0.0 `src/mac.rs`](https://docs.rs/crate/dirs/7.0.0/source/src/mac.rs)
  - Linux: parses `user-dirs.dirs`; returns `None` if the file/key is absent or
    the value is `$HOME/`; no English fallback. — [dirs 7.0.0 `src/lin.rs`](https://docs.rs/crate/dirs/7.0.0/source/src/lin.rs)
  - Windows: `SHGetKnownFolderPath` for Documents/Desktop/Downloads/Music/
    Pictures/Videos/Profile/RoamingAppData/LocalAppData. — [dirs 7.0.0 `src/win.rs`](https://docs.rs/crate/dirs/7.0.0/source/src/win.rs)
- `directories` 6.0.0: same three backends; adds `BaseDirs`, `UserDirs`,
  `ProjectDirs` structs; macOS still hardcodes, Linux still parses
  `user-dirs.dirs`, Windows still uses known folders. — [directories 6.0.0 docs](https://docs.rs/directories/latest/directories/), [directories 6.0.0 `src/mac.rs`](https://docs.rs/crate/directories/6.0.0/source/src/mac.rs)
- `known-folders` 1.4.2: Windows-only (`SHGetKnownFolderPath`), 142 variants,
  `Option<PathBuf>`, empty on non-Windows. Includes `OneDrive`, `SkyDrive`,
  `LocalDocuments`, `LocalDownloads`, `PublicDocuments`, `ProgramFiles`, etc. —
  [known-folders 1.4.2](https://docs.rs/known-folders/latest/known_folders/)
- `sysinfo` 0.33.1:
  - Linux: parses `/proc/mounts`; filters pseudo-fs, `squashfs`, `iso9660`,
    and (by default) `tmpfs`/NFS/CIFS; exposes `file_system`, `mount_point`,
    `is_removable`, `is_read_only`, `total_space`, `available_space`. —
    [sysinfo 0.33.1 `src/unix/linux/disk.rs`](https://docs.rs/crate/sysinfo/0.33.1/source/src/unix/linux/disk.rs)
  - Windows: `FindFirstVolumeW` + `GetVolumePathNamesForVolumeNameW` +
    `GetDriveTypeW`, filters to `DRIVE_FIXED`/`DRIVE_REMOVABLE`, skips volumes
    with no mount paths. — [sysinfo 0.33.1 `src/windows/disk.rs`](https://docs.rs/crate/sysinfo/0.33.1/source/src/windows/disk.rs)
  - macOS: uses `getfsstat(2)`. — [sysinfo 0.33.1 `src/unix/apple/disk.rs`](https://docs.rs/crate/sysinfo/0.33.1/source/src/unix/apple/disk.rs)
- `ignore` 0.4.33: parallel recursive walker (ripgrep's engine); defaults
  `follow_links=false`, `same_file_system=false`; defaults to hidden=true and
  honours `.ignore`, `.gitignore`, global gitignore and git exclude. —
  [ignore 0.4.33 `src/walk.rs`](https://docs.rs/crate/ignore/0.4.33/source/src/walk.rs), [ignore 0.4.33 `src/dir.rs`](https://docs.rs/crate/ignore/0.4.33/source/src/dir.rs)
- `walkdir` 2.5.0: single-threaded, `follow_links(false)` by default, loop
  detection when following, `same_file_system`, `max_depth`, `max_open`,
  `filter_entry`. — [walkdir 2.5.0 docs](https://docs.rs/walkdir/latest/walkdir/)
- `jwalk`: parallel walkdir-style iterator using Rayon, streamed sorted order,
  `process_read_dir` callback for sort/filter/skip, ~4× walkdir in the author's
  sorted-results benchmark. **The README now says "Unmaintained... Use
  dua-core instead."** — [jwalk README](https://github.com/Byron/jwalk)
- `fs_extra`: copy/move helpers with progress; no directory discovery or
  volume enumeration API. — [fs_extra docs](https://docs.rs/fs_extra/latest/fs_extra/)
- `xdg-user` 0.3.0: Linux-only; parses `user-dirs.dirs`; `UserDirs::new()`
  parses once; per-dir functions parse each call; returns
  `Result<PathBuf, Error>`; includes `XDG_PROJECTS_DIR`. — [xdg-user 0.3.0 docs](https://docs.rs/xdg-user/latest/xdg_user/)
- `windows` crate: the raw Win32 bindings needed for `SHGetKnownFolderPath`,
  `FindFirstVolumeW`, `GetVolumePathNamesForVolumeNameW`, `GetDriveTypeW`,
  `FindFirstFileExW`, `GetFileAttributesExW`, `GetFileInformationByHandle`,
  `GetFinalPathNameByHandleW`. It is not a discovery layer; you write the
  policy. — [Microsoft Learn: SHGetKnownFolderPath](https://learn.microsoft.com/en-us/windows/win32/api/shlobj_core/nf-shlobj_core-shgetknownfolderpath), [FindFirstFileExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)

### Inferences
- Minimal dependency set for Paper:
  - Keep `dirs` for the default Documents/Desktop/Downloads paths (it already
    does the right thing on Windows and Linux; macOS is fine outside a sandbox).
  - Add `known-folders` **only on Windows** (target-specific dependency) if you
    need `FOLDERID_OneDrive`, `FOLDERID_LocalDocuments` or
    `FOLDERID_PublicDocuments`. Otherwise call `dirs` and stop.
  - Keep `sysinfo` for volume capacity, but add a small mountinfo parser on
    Linux and a volume-path helper on Windows if you need precise dedupe.
  - Keep `walkdir`; do not add `jwalk` (unmaintained) or `fs_extra` (wrong
    tool). Add `ignore` only if parallel walking becomes a measured bottleneck,
    and then disable `hidden`, `ignore`, `git_ignore`, `git_global`,
    `git_exclude` and `parents`, or it will silently skip legitimate documents.
  - Do not add `xdg-user`; `dirs` already parses the same file. Add it only if
    you need `XDG_PROJECTS_DIR` or want to distinguish "file missing" from
    "key missing".
- `directories` is not a meaningful upgrade over `dirs` for this job: same
  backends, more types. Its `ProjectDirs` is useful for Paper's own cache, not
  for discovering user documents.
- `known-folders` returns `None` for virtual folders and for IDs not present on
  the system; treat `None` as "not available", not as an error.
- `sysinfo`'s Linux filtering is a good default but is **not** a security
  boundary: it reads `/proc/mounts`, so a bind mount of `/` under `/home` still
  looks like a separate mount. Use mountinfo + canonicalisation for dedupe.

### Gaps
- `known-folders`' exact behaviour under KFM (does `KnownFolder::Documents`
  return the redirected path? presumably yes, since it calls the same API) was
  not tested.
- `sysinfo` 0.33's macOS `getfsstat` filtering (does it skip hidden/system
  volumes?) was not read in full.
- `jwalk`'s exact default for `follow_links` and hidden files was not verified;
  the README only documents parallelism and sorting.

---

## Ranking heuristic for candidate roots

### Takeaway
Rank by **provenance** first (explicit user choice > OS known folder > cloud
root > home > volume root > removable/network), then apply cheap modifiers for
existence/readability, overlap and cloud-only risk. Present the ranked list to
the user; do not silently index a low-confidence root. Deduplicate by canonical
path and ancestor relationship before ranking, and again after resolution.

### Cited Findings
- Windows' known-folder API is explicitly the supported way to find "the
  current location" of Documents/Desktop/Downloads, including redirection; the
  old assumption of a fixed path under the profile is what KFM breaks. —
  [Microsoft Learn: Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/known-folders), [Working with Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders)
- On macOS, iCloud Drive's user root and CloudStorage providers are separate
  locations from `~/Documents`; the iCloud `Documents` entry can be a symlink
  back to the real `~/Documents`, so treating both as independent roots
  double-counts. — [cver.net](https://cver.net/devlog/unify-mac-ios-downloads), [Eclectic Light](https://eclecticlight.co/2023/07/17/how-does-icloud-work-plus-a-new-version-of-cirrus)
- On Linux, `user-dirs.dirs` can point a "known" folder at an absolute path
  outside `$HOME`, so a Documents candidate can legitimately live on another
  volume. — [user-dirs.dirs(5)](https://man.archlinux.org/man/user-dirs.dirs.5.en)
- Windows KFM can place Documents/Desktop under OneDrive, while the old local
  folders remain; only the known-folder path is current. — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/2085166/what-are-the-known-folder-paths-returned-by-shgetk), [Microsoft Learn: Working with Known Folders](https://learn.microsoft.com/en-us/windows/win32/shell/working-with-known-folders)
- macOS TCC protects removable and network volumes as well as Documents/
  Desktop/Downloads, so "external volume" is not a permission-free higher rank.
  — [Eclectic Light: Explainer: Permissions, privacy and TCC](https://eclecticlight.co/2025/11/08/explainer-permissions-privacy-and-tcc)
- Windows `GetDriveTypeW` distinguishes fixed/removable/remote, and
  `sysinfo` uses that to exclude optical/RAM/unknown volumes. — [GetDriveTypeW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getdrivetypew), [sysinfo Windows disk](https://docs.rs/crate/sysinfo/0.33.1/source/src/windows/disk.rs)
- Cloud-only files are identifiable by `RECALL_ON_DATA_ACCESS` /
  `RECALL_ON_OPEN` / `OFFLINE`, and `FIND_FIRST_EX_ON_DISK_ENTRIES_ONLY`
  restricts enumeration to physically present files. — [File Attribute Constants](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants), [FindFirstFileExW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-findfirstfileexw)

### Proposed scoring model
Base score by provenance (higher wins):

| Rank | Candidate | Base |
|---|---|---|
| 1 | Explicit `--root` / config / MCP `add_root` | 1000 |
| 2 | Windows `FOLDERID_Documents`; Linux `XDG_DOCUMENTS_DIR`; macOS `~/Documents` (incl. iCloud-enrolled) | 900 |
| 3 | Windows `FOLDERID_Desktop`; Linux `XDG_DESKTOP_DIR`; macOS `~/Desktop` | 800 |
| 4 | Windows `FOLDERID_Downloads`; Linux `XDG_DOWNLOAD_DIR`; macOS `~/Downloads` | 700 |
| 5 | Cloud container roots: `FOLDERID_OneDrive`, `~/Library/Mobile Documents/com~apple~CloudDocs`, `~/Library/CloudStorage/<provider>` | 600 |
| 6 | Other known folders: Pictures, Music, Videos, Public, Projects | 400–500 |
| 7 | User home (`FOLDERID_Profile`, `$HOME`) | 200 |
| 8 | Fixed volume root / volume mount point | 100 |
| 9 | Removable volume | 0 (opt-in) |
| 10 | Network volume / share | −100 (opt-in) |

Modifiers (additive, clamped):

- `+40` path exists and is a directory.
- `+30` path is readable (a cheap `read_dir` of the first entry only; on macOS
  this can trigger TCC, so skip the probe until the user opts in).
- `−200` path is inside a system/tooling directory (`Windows`, `Program Files*`,
  `ProgramData`, `AppData`, `Library/Caches`, `.cache`, `node_modules`, `.git`,
  `target`).
- `−150` path is a volume root with no `Documents`/`Users`/`Desktop` child
  (cheap top-level probe capped at ~50 entries).
- `−100` filesystem is network (`DRIVE_REMOTE`, `nfs`, `cifs`, `sshfs`, `9p`).
- `−150` filesystem is read-only or removable (unless explicitly opted in).
- `−100` cloud-only ratio is high (sample first N entries; if
  `RECALL_ON_DATA_ACCESS`/`OFFLINE` > 50%, warn).
- `−300` path is a macOS TCC-protected location and consent is not granted.
- `+20` path contains at least one indexable document in its top level (capped
  probe; never recursive).
- `+10` path is on the same physical device as the OS/user profile (locality).

Deduplication rules (apply before and after scoring):

1. Canonicalise each path (`realpath` on Unix, `GetFinalPathNameByHandle` or
   `fs::canonicalize` on Windows). Drop duplicates.
2. If root A is an ancestor of root B and both are selected, keep A and drop B
   **unless** B has a higher provenance score (e.g. explicit user root B inside
   home A). Explicit always wins.
3. For Windows volumes, dedupe by volume GUID, not mount point; keep the
   highest-ranked mount point per volume.
4. For macOS iCloud, if `~/Documents` has the CloudDocs File Provider xattr,
   drop the `com~apple~CloudDocs/Documents` symlink candidate (it resolves to
   the same inode/path).
5. For Linux bind mounts/subvolumes, dedupe by `(major:minor, root)` and by
   canonical path; keep the shortest, most user-meaningful mount point.

Deterministic tie-break: higher base score, then shorter path, then
case-insensitive lexicographic order. This makes the CLI output stable across
runs and platforms.

### Inferences
- The ranking should be **advisory**, not automatic. A local-first indexer that
  silently crawls `D:\` because it scored 100 will surprise users and burn I/O.
  Show the list, mark `default_selected` for ranks 1–5 that exist and are
  readable, and require an explicit opt-in for volume roots, removable and
  network.
- On Windows with KFM, the user's mental model is "Documents is in OneDrive".
  Rank `FOLDERID_Documents` (900) above `FOLDERID_OneDrive` (600) so the
  specific folder wins; then drop the nested OneDrive root if Documents is
  already selected, or drop Documents if the user explicitly selected the
  OneDrive root (they may want the whole cloud tree).
- On macOS with Desktop & Documents sync, `~/Documents` and `~/Desktop` remain
  the correct user-facing roots. Do not replace them with the iCloud path; add
  the iCloud root only as an additional container for files that are not under
  Desktop/Documents (e.g. app-specific iCloud folders).
- On Linux, a `user-dirs.dirs` entry pointing at `/mnt/data/Docs` should outrank
  the generic `/mnt/data` volume root, because the user explicitly configured
  it. That is already captured by provenance rank 2 vs 8.
- Recommended candidate record:
  ```rust
  struct Candidate {
      path: PathBuf,
      kind: RootKind,          // Documents, Desktop, Downloads, Cloud, Home, Volume, ...
      source: RootSource,      // Explicit, KnownFolder, XdgUserDir, Heuristic, VolumeEnum
      base_score: i32,
      rank: i32,
      exists: bool,
      readable: Option<bool>,  // None = not probed (TCC/consent)
      writable: Option<bool>,
      fs_kind: FsKind,         // Local, Network, Removable, ReadOnly, Pseudo
      cloud: CloudKind,        // None, ICloud, OneDrive, GoogleDrive, Dropbox, Other
      cloud_only_ratio: Option<f32>,
      requires_consent: bool,  // macOS TCC
      notes: Vec<String>,      // "KFM redirected", "XDG value outside $HOME", ...
  }
  ```
- Recommended pipeline:
  1. Resolve explicit config/CLI roots (rank 1000).
  2. Resolve OS known folders (Windows `SHGetKnownFolderPath`; Linux
     `user-dirs.dirs`; macOS `dirs` + iCloud xattr check).
  3. Resolve cloud containers (`FOLDERID_OneDrive`,
     `~/Library/Mobile Documents/com~apple~CloudDocs`,
     `~/Library/CloudStorage/*`) and mark them as containers.
  4. Enumerate volumes (Linux mountinfo, Windows FindFirstVolume, macOS
     `getfsstat`) and classify local/removable/network/read-only.
  5. Canonicalise, dedupe by ancestor/volume/inode, apply modifiers, sort.
  6. Probe readability only for candidates the user selects (or for all
     non-TCC candidates at startup; never probe TCC paths without consent).
  7. Index in rank order, one root at a time, with per-root budgets and
     resumable state; stop early if the user has already found what they need.
- The tool should persist the user's accepted root set separately from the
  discovered candidates, so a future macOS TCC denial or a disconnected drive
  does not silently change what is indexed.

### Gaps
- No published, authoritative ranking heuristic exists for this exact problem;
  the table above is a synthesis of the provenance facts, not a standard.
- The "cloud-only ratio" sample can itself hydrate data on some providers; it
  must be implemented with metadata-only APIs (Windows attributes; macOS
  File Provider resource keys) or skipped.
- No data on how often KFM is enabled on consumer Windows 11 versus manual
  redirection; the ranking assumes KFM is common enough to be the default
  explanation for a `Documents` path under OneDrive.
- The interaction between macOS TCC consent and a long-running background
  indexer (does consent persist per binary, per responsible app, or per
  session?) was not fully established; grants are per code-signing identity and
  can be invalidated by updates/re-signing per the Orca report.
