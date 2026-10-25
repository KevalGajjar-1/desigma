# Live collaboration — design and plan

Status: phases 1-4 done (core sync, transport, app integration, presence); the headless server is done; the rest of polish is open. Target: 2–10 people editing one file live, on Windows, macOS and Linux,
with no account and no server to run.

## How it works for users

1. The host opens a file and clicks **Share**. Desigma shows a link
   `desigma://join/<ticket>` and, optionally, a password. The host picks whether guests can
   edit or only view, and whether each joiner needs approval.
2. A guest clicks the link (or pastes it into **Join session** on the home screen), enters a
   name and the password, and the file opens with everyone's cursors on it.
3. The session lasts while the host keeps the file open. When it ends, guests keep a local
   copy they can save.

## Architecture

```
guest ─┐                 ┌─ guest
guest ─┼── iroh QUIC ── HOST ── iroh QUIC ──┼─ guest
guest ─┘   (encrypted,   (orders every edit) └─ guest
           P2P, relay fallback)
```

* **Star topology.** Every guest has one connection to the host. Guests never talk to each
  other. The host is the single authority: it numbers every change and broadcasts it, so all
  peers converge on the same document.
* **Transport: [iroh](https://github.com/n0-computer/iroh).** Pure Rust QUIC, dialled by public
  key, with NAT hole punching and public relays (over HTTPS) as fallback. The ticket in the link
  holds the host's endpoint id plus its addresses. No ports to open, no tunnel binary.
  ALPN `desigma/collab/1`.
* **Encoding.** JSON frames, length-prefixed, zstd for large ones. The document model uses
  `skip_serializing_if`, so it needs a self-describing format.

## Syncing the document

### Changes are found by diffing, not by hooking every edit path

Edits reach `App::doc` from many places: `Tx`/`History`, live drags and scrubs (which change the
document every frame before committing), the agent bridge, plugins, version restore. Instead of
instrumenting each one, the sync layer keeps a **shadow** copy of the last synced state. After
any frame where the document changed (and at most ~20 times a second), it diffs `doc` against the
shadow and turns the difference into **ops**. This also means other people see drags live, not
just when the mouse is released.

Ops (crate `desigma-collab`):

| Op | Meaning |
|---|---|
| `Create { node, parent, after }` | New node with all fields, placed after sibling `after` (or first). |
| `Patch { id, fields }` | Changed top-level `Node` fields, as `field name → JSON value`. |
| `Move { id, parent, after }` | Reparent or reorder. |
| `Delete { id }` | Remove a node and its subtree. |
| `DocPatch { fields }` | Changed document-level fields (styles, variables, comments, flows, guides, timelines, pages order, board session…). |
| `Image { hash }` | A new embedded image is available; bytes are fetched on demand. |

The diff only compares nodes that changed since the last diff; unchanged ones are equal by
`PartialEq` and skipped.

### Conflict rules

* **Properties: last writer wins, per field.** Two people changing the same layer's fill and
  position both win. The same field: the host's order decides.
* **Optimistic local edits.** A guest applies its own edits immediately and sends them. While a
  guest has unacknowledged changes to a field, incoming changes to that field are held back, so
  values don't flicker back and forth. The host's acknowledgement then settles the final value.
* **Structure is validated by the host.** Moving a node under its own descendant, into a
  non-container, or under a deleted parent is rejected. The host then sends the guest a
  correction: the current state of the affected nodes.
* **Order uses sibling anchors** ("after X"), so two people inserting into the same frame both
  keep their layers.
* **Delete wins over edit.** Patches to a deleted node are dropped.

### Node ids

`NodeId` is a counter, so two peers would hand out the same ids. On join, each peer gets an index
(host 0, guests 1, 2, …) and allocates ids in its own range `(peer << 40) | n`. Existing ids keep
working.

### Undo

Each person's undo reverts only their own changes. When a remote patch arrives, the matching
fields in the stored inverse commands of the local `History` are updated to the remote values.
Undoing an earlier local edit then does not revert someone else's newer change. An undo is an
ordinary local edit, so it syncs through the diff like any other.

### Joining

The host sends a snapshot: the document without image bytes, plus the current op sequence
number. The guest then requests images by hash, and caches them by content hash between
sessions. Fonts the guest is missing are sent if the host has the file, otherwise flagged as
missing.

## Presence (not part of the document)

Sent separately, not sequenced, last state wins, about 20 Hz:

* cursor position and page;
* selection;
* viewport (for **Follow**);
* name, colour, role.

On the canvas: coloured cursors with name tags and selection outlines in each person's colour.
The top bar gets an avatar row; clicking an avatar follows that person.

## Security

* The ticket is the secret: only people with the link can find the host.
* Optional password. It is checked over the already-encrypted connection, compared in constant
  time, with failed attempts rate-limited.
* The host can require approval for each joiner, kick anyone, and end the session.
* Roles: **can edit** / **can view**. The host drops ops from viewers.
* Off until the host clicks Share. Nothing listens otherwise.

## Platforms

iroh runs on Windows, macOS and Linux; the protocol is the same everywhere. The
`desigma://` link handler is registered per OS: Inno Setup registry keys on Windows,
`CFBundleURLTypes` in `Info.plist` on macOS, `x-scheme-handler` in the `.desktop` file on Linux.
Pasting the link always works as a fallback. See "Links and files reach the running editor" below.

Every peer must speak the same protocol version. A mismatch is refused with "update desigma".

## Limits of the first version

* The session ends when the host closes the file.
* Joining needs the desktop app (no browser client yet).
* About 2–20 people per session, limited by the host's upload bandwidth when people join.

## Phases

1. **Core sync (`crates/desigma-collab`, no network).**
   * Ops, diff and apply.
   * Id ranges.
   * Host sequencer with structure validation.
   * Guest reconciliation (pending fields, corrections).
   * History rebase.
   * Snapshot.
   * In-process tests with simulated peers, including randomized "everyone edits at once" runs
     that must converge to identical documents.
2. **Transport (`crates/desigma-net`).**
   * iroh endpoint, ticket and link encoding, and the handshake (protocol version, password,
     name, approval).
   * Framed messages, peer join and leave events, broadcast.
   * A background tokio runtime with channels to the UI thread.
   * Loopback tests with two endpoints in one process (no relay).
3. **App integration (done).**
   * Share and Join dialogs and the session lifecycle.
   * Applying remote ops to `App::doc`, plus the shadow and diff loop.
   * Roles, kick and end session.
   * The `desigma://` handler on all three OSes.
4. **Presence (done).** Cursors, selections, avatars, Follow.
5. **Polish.** Font transfer, image cache, reconnect after a dropped connection, and a headless
   `desigma-server` that can host a file permanently (same protocol).

## Implementation notes (phases 3 and 4)

Code: `crates/desigma-app/src/collab/` (`mod.rs` session and per-frame pump, `link.rs` transport traits and
the desigma-net adapters, `presence.rs`, `ui.rs`, `draw.rs`, `tests.rs`).

* **Frame loop.** `collab::pump` runs in `Shell::logic` (also while the window is occluded). Drain the
  transport, hand guest messages to the sync core, number our own edits when the canvas revision changed
  (at most 20 Hz, flushed once idle), send presence, ease remote cursors, follow.
* **Wire.** Reliable lane carries `ToGuest { Sync(ServerMsg), Role }` and `ToHost { Sync(ClientMsg) }` as JSON;
  desigma-net deflates big frames. The host fans out itself (per peer), so a joiner only gets messages after
  its snapshot. Presence is a datagram of at most 1000 bytes (selection cut to fit); the host stamps the sender
  and relays it to the others, replays known presence to a newcomer, and sends `gone` when someone leaves.
  Silent people expire after 12 s; keepalives go out every 2 s.
* **Transport traits.** `HostLink` / `GuestLink` wrap desigma-net so tests can use in-memory channels.
  `DESIGMA_COLLAB_NETWORK=direct` skips relays (LAN, loopback).
* **Undo.** Guest messages go through `rebase_history`; the host rebases its own history for guest ops. An undo
  that fails with NotFound after a remote delete is silent.
* **Read-only.** `App::read_only()` (old version or view-only guest) gates `run_tx` and the canvas. The host
  still rejects viewer ops and corrects the guest.
* **Ids.** The app only allocates through `Document::alloc_id`; nothing resets `next_id`. A shared document
  keeps its id range until the session ends. Viewing or restoring old versions is refused while hosting.
* **Joining.** The shared file replaces the open document (like File, Open), is not tied to a path, and
  "Save a copy" uses Save to library. The display name is the "Your name" setting (OS user name by default).
* **Link scheme.** `desigma desigma://join/...` opens Join prefilled. Registered by the Windows installer
  (`scripts/windows/desigma.iss`), `CFBundleURLTypes` (macOS) and `x-scheme-handler/desigma` (Linux).

* **Links and files reach the running editor.** On macOS, links and Finder opens arrive as Apple events,
  which winit does not surface; `src/macos.rs` registers an `NSAppleEventManager` handler (`GURL` and
  `odoc`) at startup and again once the app has launched. On Windows and Linux a second
  `desigma desigma://join/...` (or `desigma FILE`) process connects to the running editor and exits without
  a window (`src/instance.rs`): a `127.0.0.1` listener on an ephemeral port, with the port and a random token
  in `instance.json` (per-user runtime or local data dir, 0600 on unix). It accepts only
  `<token> open-link desigma://join/...`, `open-file <absolute path>` and `ping`, in lines of at most
  4 KiB. A stale file (connection refused) just means a normal start. Both paths feed one inbox that the UI
  drains each frame: Join opens prefilled, a file opens like a dropped one (both ask "Discard unsaved changes?" with Save / Discard / Cancel first when the editor has unsaved edits or a live session), and the window is
  un-minimized and focused (or asks for attention when the OS refuses focus).

Known gaps: the macOS handlers are compiled in CI but untested by hand; the board has a floating
Share strip instead of the inspector row; fonts and reconnect are phase 5.

## Phase 5: `desigma-server`

Code: `crates/desigma-server` (library plus the `desigma-server` binary; usage, systemd unit and security
notes in its README).

* **Same protocol.** The server is a headless desktop host: a `HostSession` over a desigma-net `Host`, with the
  same `ToGuest` / `ToHost` JSON, snapshot on join, `Dest` routing and presence relay as
  `desigma-app/src/collab/mod.rs`. That loop is tied to app state (toasts, undo rebase, remote cursors), so it
  is repeated in about 60 lines instead of being extracted. The app joins it with no changes.
* **Authority and persistence.** It applies guests' ops to its own document, autosaves when changed (default
  10 s) and on Ctrl-C / `SIGTERM`, always temp file plus rename, with rotating `FILE.bak.N` backups.
  `.fig` / REST `.json` / `.svg` inputs are imported once and saved as `.desigma` next to the source.
* **Stable link.** `HostOptions` gained `secret_key` (fixed endpoint identity) and `port`; the server keeps the
  key in `.<name>.desigma-key` (mode 0600 on Unix). `Ticket::relay_only` drops direct addresses so the link
  does not change when the IP does.
* **No approval.** Nobody is there to click Accept, so everyone passing the password is let in; the password
  (preferably via `--password-env`) is the gate, and `--view-only` makes everyone a viewer.
