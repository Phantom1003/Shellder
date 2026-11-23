# Source reference

The pages of this book can point into the source in three ways.

**The API reference.** Every module, type and function of the Rust side,
private ones included, as `rustdoc` reads them from the source, is
published beside this book: [the `shellder` crate](../api/shellder/index.html).
A page links to an item there, for example
[`core::pty`](../api/shellder/core/pty/index.html), the controlling terminal
and the reader that answers ssh's questions.

**A piece of the source, as it is.** A page can show lines of a file of
the repository, read when the book is built, so that the page cannot fall
behind the code. This is the rule a locked host is brought back by, from
`src-tauri/src/core/reconnect.rs`:

```rust,ignore
{{#include ../../../src-tauri/src/core/reconnect.rs:1:24}}
```

**A link to the file.** For the pages' scripts in `ui/`, which have no
reference of their own:
[`ui/console.js`](https://github.com/Phantom1003/Shellder/blob/main/ui/console.js).

How the repository is laid out, how it is tested and built is in
[`AGENTS.md`](https://github.com/Phantom1003/Shellder/blob/main/AGENTS.md).
