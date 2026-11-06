# GiRustStatus

Portable Windows app that tracks GitHub/GitLab repos you follow, shows new
releases, and checks every download on VirusTotal before it reaches your
download folder. It never installs anything; you run the files yourself.

## Run

Copy `giruststatus.exe` anywhere and start it. It keeps `config.toml` and
`repos.json` next to the exe, plus `tmp/` and `downloads/`. No installer,
no registry.

## First setup (Settings)

- **VirusTotal API key**: free key from virustotal.com. Without it every
  download asks "Not scanned - proceed?" (Cancel is the default).
- **GitHub/GitLab token**: optional. Raises GitHub from 60 to 5000
  requests/hour.

## Where your keys live

The three keys are stored in **Windows Credential Manager** (entries
`vt_api_key.GiRustStatus`, `github_pat.GiRustStatus`,
`gitlab_pat.GiRustStatus`), never in `config.toml` or a plain export.
This is the one thing outside the app folder. Consequences:

- Copies of the app on the same Windows account share the keys.
- Keys do **not** travel with the folder.

## Moving to another PC

1. Settings, **Export (encrypted, with keys...)**, choose a password. This
   writes `girust-export.enc` next to the exe.
2. Copy the folder (with that file) to the new PC.
3. Settings, **Import...**, enter the password. Repos, state and keys come
   across; the keys land in the new PC's Credential Manager.

The plain export has no keys. Delete the `.enc` file after importing.

## Download safety

Download, hash lookup on VirusTotal, upload for scanning if unknown.
Clean files are delivered automatically. Flagged or unscanned files show
a blocking dialog with Cancel as the default (Enter or Esc).

## Build

`cargo build --release`, output `target/release/giruststatus.exe`.
