# Installing desigma

Download the archive for your platform from the
[latest release](https://github.com/deepfounder-ai/desigma/releases):

| Platform | File |
|---|---|
| Windows (x64) | `desigma-<version>-x86_64-pc-windows-msvc.zip` |
| macOS (Apple Silicon) | `desigma-<version>-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `desigma-<version>-x86_64-apple-darwin.tar.gz` |
| Linux (x64) | `desigma-<version>-x86_64-unknown-linux-gnu.tar.gz` |

Each archive holds the `desigma` editor, the `desigma-diff` tool, the licenses
and the README. `SHA256SUMS` on the release page lets you check the download:

```sh
sha256sum -c SHA256SUMS --ignore-missing      # Linux
shasum -a 256 -c SHA256SUMS --ignore-missing  # macOS
```

## The first-launch warning

desigma's release builds are **not code-signed yet**, so Windows and macOS warn
the first time you open them. The warning is about the missing publisher
signature, not about anything the app does: the source is public and every
release is built by GitHub Actions from a tagged commit.

### Windows

1. Unzip the archive and double-click `desigma.exe`.
2. SmartScreen shows **"Windows protected your PC"**. Click **More info**.
3. Click **Run anyway**. Windows remembers the choice for this file.

### macOS

The archive contains a plain executable, so macOS quarantines it as a file
downloaded from the internet.

Either open it once and allow it:

1. Unpack the archive and double-click `desigma` (or run `./desigma` in Terminal).
2. macOS says it cannot verify the developer. Click **Done** (or **Cancel**).
3. Open **System Settings → Privacy & Security**, scroll to the message about
   `desigma`, and click **Open Anyway**. Confirm with your password.

Or remove the quarantine flag in Terminal, from the unpacked folder:

```sh
xattr -d com.apple.quarantine ./desigma ./desigma-diff
./desigma
```

### Linux

No warning. Unpack, make sure the system libraries are installed (most desktops
have them already), and run it:

```sh
tar -xzf desigma-<version>-x86_64-unknown-linux-gnu.tar.gz
./desigma
```

On a minimal system install the libraries the editor links against, for
example on Debian or Ubuntu:

```sh
sudo apt install libxkbcommon0 libwayland-client0 libx11-6 libxcursor1 libxrandr2 libxi6 libgl1 libfontconfig1
```

## GPU rendering

The canvas draws with the GPU (Vulkan, Metal, DirectX 12 or OpenGL) and falls
back to the CPU renderer when no suitable adapter is found. If you see drawing
problems, turn off **Preferences → Use GPU rendering**.

## Building from source

```sh
git clone https://github.com/deepfounder-ai/desigma
cd desigma
cargo run --release -p desigma-app
```

The pinned Rust toolchain installs automatically through `rust-toolchain.toml`.
See [CONTRIBUTING.md](../CONTRIBUTING.md) for the Linux build packages and the
test workflow.

## Signed builds

Signing is planned: Windows binaries through a code-signing certificate, macOS
builds as a notarized `desigma.app`. Once that lands, these warnings go away and
this page will say so.
