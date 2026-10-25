# Live collaboration — design and plan

Status: planned. Target: 2–10 people editing one file live, on Windows, macOS and Linux,
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
Pasting the link always works as a fallback.

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
3. **App integration.**
   * Share and Join dialogs and the session lifecycle.
   * Applying remote ops to `App::doc`, plus the shadow and diff loop.
   * Roles, kick and end session.
   * The `desigma://` handler on all three OSes.
4. **Presence.** Cursors, selections, avatars, Follow.
5. **Polish.** Font transfer, image cache, reconnect after a dropped connection, and a headless
   `desigma-server` that can host a file permanently (same protocol).
