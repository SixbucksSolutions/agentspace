# agentspace's crates

RFC 2119 keywords; the capitals are load-bearing.

**One crate per area of the Rust rewrite, each in a folder of its own here** (D38 in
[docs/decisions.md](../docs/decisions.md)). The workspace at the repository's root takes every
folder here as a member, so a story adds its crate without editing the root.

- **A story MUST change only the crates its card names**, so the stories of one turn never touch
  the same folder.
- **The rewrite is built from the specs alone** (D20): [docs/lease-api.md](../docs/lease-api.md)
  and `schemas/`, [docs/state.md](../docs/state.md), [docs/driver.md](../docs/driver.md), and the
  rest of `docs/` as each card cites it. The prototype's code under `scripts/` and `bin/` MUST NOT
  be read.
- **Which crate holds what** is D38's table.
