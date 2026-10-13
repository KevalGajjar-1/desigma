# Desigma MCP

A local MCP server for inspecting and editing design files with Desigma's document model and code generators. This first version runs an isolated, in-memory file session over stdio. It does **not** attach to, inspect the selection of, or change an open Desigma editor window.

## Build and connect

From the repository root:

```sh
cargo build --locked -p desigma-mcp
```

The executable is `target/debug/desigma-mcp` (`desigma-mcp.exe` on Windows). Use `--release` for a release build and change `debug` to `release` in the paths below. Rust and the repository's normal build prerequisites are required only for building.

Configure an MCP client that supports local stdio servers. These are generic `mcpServers` examples; use the configuration format and location required by your client. Replace both paths with absolute paths on your machine. The workspace directory must already exist.

Windows:

```json
{
  "mcpServers": {
    "desigma": {
      "command": "C:\\path\\to\\desigma\\target\\debug\\desigma-mcp.exe",
      "args": ["--workspace", "C:\\path\\to\\designs"]
    }
  }
}
```

macOS/Linux:

```json
{
  "mcpServers": {
    "desigma": {
      "command": "/path/to/desigma/target/debug/desigma-mcp",
      "args": ["--workspace", "/path/to/designs"]
    }
  }
}
```

The client launches the process and speaks MCP over stdin/stdout. The server does not listen on a network port. No account, API key, Figma connection, or running editor is needed. Configuration is not installed automatically.

## File workflow

1. Call `desigma_open_file` with a path inside the configured workspace, or `desigma_new_document` with a name.
2. Use `desigma_get_document` to discover page IDs, then `desigma_list_nodes` and `desigma_get_node` to inspect layers. IDs are positive integers; obtain them from tool results rather than Figma node IDs.
3. Inspect variables or generate handoff code. Make edits with the creation, update, and deletion tools; undo and redo operate on this session's edit history.
4. Call `desigma_save_file` with an **unused** `.desigma` filename. Its parent directory must already exist. Open the resulting file in the desktop editor to continue there.

Example tool arguments:

```text
desigma_open_file     {"path":"input.fig"}
desigma_get_document  {}
desigma_list_nodes    {"query":"button","limit":25}
desigma_get_node      {"node_id":42}
desigma_generate_code {"node_id":42,"language":"react"}
desigma_update_node   {"node_id":42,"name":"Primary action","width":160}
desigma_save_file     {"path":"output-v2.desigma"}
```

`42` is illustrative: substitute an ID returned by the preceding queries. Relative paths resolve beneath `--workspace`. Absolute paths are accepted only within that same directory tree. Reads canonicalize paths and reject escapes; writes resolve the existing parent and create a new file without overwriting an existing file or symlink.

Edits remain in memory until saved. Restarting the server loses unsaved work. Opening another file or starting a new document resets undo history and refuses to discard pending edits unless `discard_changes: true` is explicitly supplied. Save each revision under a new filename; overwriting is not supported. Tool failures return an MCP error result with an explanatory message.

Imports are limited to 64 MiB per file, 100,000 layers, and 256 levels of nesting. The server validates the imported document graph before using it, rejecting malformed parent/child relationships and cycles.

## Tools

| Tool | Purpose |
| --- | --- |
| `desigma_open_file` | Import a native `.desigma`, `.fig`, SVG, or supported JSON design into the session. |
| `desigma_new_document` | Start an empty document with one page. |
| `desigma_save_file` | Write the session to a new native `.desigma` file. |
| `desigma_get_document` | Read document name, pages, counts, and undo/redo availability. |
| `desigma_list_nodes` | Search names case-insensitively or list direct children; paginate using `next_offset`. |
| `desigma_get_node` | Read a complete native node, including child IDs, paints, geometry, text, and bindings. |
| `desigma_get_variables` | Read collections and tokens; optionally resolve one node's direct bindings in its inherited modes. |
| `desigma_generate_code` | Return CSS, Tailwind, React, SwiftUI, Compose, or SVG source without writing a file. |
| `desigma_create_node` | Add a frame, rectangle, ellipse, or text layer to a container. |
| `desigma_update_node` | Change name, position, dimensions, visibility, opacity, or plain text. |
| `desigma_delete_node` | Remove a non-page layer and its descendants. |
| `desigma_undo` / `desigma_redo` | Traverse the session's edit history. |

`desigma_list_nodes` defaults to 100 results and accepts 1–500 per call. `parent_id` filters direct children, not all descendants. Coordinates are relative to the parent. Width and height must be non-negative, geometry must be finite and within ±1,000,000, and opacity must be between 0 and 1. Replacing text clears rich-text runs and list ranges. Page roots cannot be edited or deleted by the layer tools.

## Current limits

The server shares Desigma's importers, document types, variables, undo commands, and exporters. Code generation resolves bound variable values. Import and generated-code fidelity follow the underlying Desigma implementations; generated source is a handoff starting point.

Layer edits do not run the desktop application's auto-layout reflow, text measurement, connector rerouting, or component-instance synchronization. Do not assume that resizing a container rearranges its children or that editing a component updates every instance. Rendering screenshots, editing token definitions or arbitrary paint properties, and live editor selection are not exposed in this version.

A future live adapter should dispatch requests onto the editor thread and apply mutations through the application's transaction APIs, preserving layout and component behavior and the user's undo history. The present server has no live adapter.

## Verification and evaluation

```sh
cargo test --locked -p desigma-mcp
```

`tests/evaluation.xml` contains ten independent, read-only retrieval questions based on the repository's fixed Figma REST fixture. For each question, start a fresh server with the repository root as `--workspace`, then preload `crates/desigma-fig/tests/fixtures/rest_file.json` using `desigma_open_file`. This preload is test setup; answering the questions requires only the read-only query/code tools. Do not create, update, delete, undo, redo, or save during evaluation. Match the response against the XML answer exactly.
