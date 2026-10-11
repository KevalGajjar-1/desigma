# Bundled fonts

## Inter (`inter/`)

| File | Bytes | Notes |
| --- | --- | --- |
| `Inter[opsz,wght].ttf` | 876576 | variable, axes `opsz` 14-32 and `wght` 100-900 |
| `Inter-Italic[opsz,wght].ttf` | 906596 | variable italic, same axes |
| `OFL.txt` | 4377 | license text |

- Source: the official Google Fonts repository, `https://github.com/google/fonts/raw/main/ofl/inter/` (branch `main`, downloaded 2026-10-10).
- Font version: 4.001 (`Version 4.001;git-66647c0bb`, Inter by The Inter Project Authors, https://github.com/rsms/inter). Fontsource lists the same family as `v20`, last modified 2025-09-10.
- License: SIL Open Font License 1.1 (`inter/OFL.txt`). Copyright 2020 The Inter Project Authors.

Embedded into the binary with `include_bytes!` in `src/assets.rs`.
