# Rules for agents working on support-fins-rs

You are porting a working, battle-tested JavaScript engine to Rust. The JS is the
specification. Your job is to reproduce it, not to improve it. Read this whole file before
you start a ticket, and again whenever a review sends you back.

## The source of truth

- **JS reference checkout:** `~/d/storskegg/support-fins` (read-only for you). It must be at
  the commit in `UPSTREAM` (`git -C ~/d/storskegg/support-fins rev-parse HEAD`). If it isn't,
  stop and say so in your ticket. Never check out a different commit there.
- **The plan:** `docs/PORT-PLAN.md`. **Parity format:** `docs/PARITY.md`. **Test map:**
  `docs/TEST-MAP.md`.
- When this file, your ticket and the JS disagree: the ticket's scope wins for *what* to do,
  this file wins for *how*, and the JS wins for *what the code computes*.

## Workflow

1. Work in your own worktree, never in the main checkout:
   ```sh
   git -C ~/RustRoverProjects/support-fins-rs fetch origin
   git -C ~/RustRoverProjects/support-fins-rs worktree add ~/RustRoverProjects/sfrs-wt/<ticket#>-<slug> -b t<ticket#>-<slug> origin/main
   ```
   Do all your work, builds and commits inside that directory.
2. Touch only the files your ticket lists. If you need to change a file outside that list
   (a shared `Cargo.toml`, another agent's module, a signature the orchestrator wrote),
   **stop and ask** in a comment on your issue (`gh issue comment <n> --repo storskegg/support-fins-rs`).
   Don't work around it.
3. Before opening the PR, run `just check` in your worktree. It must be fully green: fmt,
   clippy with `-D warnings`, every test, and parity.
4. Commit in small logical steps. Every commit message ends with:
   ```
   Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>
   ```
5. Push your branch and open a PR against `main` with `gh pr create --repo storskegg/support-fins-rs`.
   Use the PR body template below, put `Closes #<ticket>` in it, and end the body with:
   ```
   🤖 Generated with [Claude Code](https://claude.com/claude-code)
   ```
6. Report back with the PR URL, a three-line summary, and every open question.
   **Do not merge.** The orchestrator reviews and merges.
7. When review sends changes back, fix them on the same branch, push, and reply to each
   review comment saying what you changed. Don't open a new PR.

## How to port (the rules reviews enforce)

1. **Port; don't redesign.** Keep the JS function's structure, its branch order, its loop order
   and its early returns. Names become `snake_case` (`surfaceZAt` → `surface_z_at`) and keep their
   meaning. Carry over the JS comments that explain *why*: they're the reasoning behind
   thresholds, and reviewers check that they survive.
2. **Cite the origin.** Every ported item's doc comment starts with its JS origin, for example
   ``/// Port of `surfaceZAt` (web/prop/surface.js:120-171).`` Line numbers are at the pinned commit.
3. **Never invent.** Don't invent constants, thresholds, epsilons, branches, fallbacks or error
   cases. If the JS doesn't do it, neither does the Rust. If the JS is ambiguous, or relies on
   behaviour you can't reproduce exactly, **stop and ask** on the issue. A wrong guess costs a
   review round. A question costs a minute.
4. **Do arithmetic exactly as JS does.**
   - `f64` everywhere JS has a number. Keep JS's expression shape and evaluation order:
     `a * b + c` stays `a * b + c`. Never use `mul_add`. Never reassociate, factor or "simplify"
     an expression. Never hoist a division into a reciprocal multiply.
   - Every `Math.*` call, `| 0`, `>>> 0`, `Math.imul`, `Number(s)`, `parseFloat`, `String(x)`,
     `` `${x}` `` and `toFixed` goes through `sf_engine::jsnum`. **Never** use the `std` versions
     (`f64::round`, `f64::sin`, `f64::min`, `as i32`, `format!("{}", x)`, …) for something JS
     computed.
   - Data stored in a `Float32Array` was rounded to f32 when it was stored. Reproduce that at
     the same point: `(v as f32) as f64`. In a `Float64Array` it wasn't. Check which one the
     JS uses every time.
   - `Int32Array` and `Uint32Array` wrap on store. Reproduce that with `jsnum::to_int32` and
     `jsnum::to_uint32`.
5. **Do order exactly as JS does.** `Map`, `Set` and object keys iterate in insertion order:
   use `IndexMap` and `IndexSet`, never `HashMap` for anything you iterate. `Array.sort` is
   stable: use `sort_by` (stable), never `sort_unstable*`. A JS comparator that returns NaN,
   or that `a - b` overflows, must be reproduced as JS treats it. Ask if you're unsure.
6. **No global mutable state.** JS's module-level config objects (`PROP`, `FIN`, …) and caches
   (`WeakMap`s, module `let`s) become explicit parameters or owned caches, as your ticket's
   signatures specify. No `static mut`, no `thread_local!` state, no `lazy_static` config.
7. **No `unsafe`.** The workspace denies it. If you think you need it, ask.
8. **No performance work** before milestone M5, beyond not being needlessly quadratic where JS
   isn't. No SIMD, no Rayon, no caching JS doesn't do, no restructuring "for speed".
9. **Don't change signatures you were given.** The orchestrator owns public interfaces. If one
   is wrong or can't express the JS, ask.
10. **No new dependencies** beyond `[workspace.dependencies]` and the ones your ticket names.

## Tests (the acceptance gate)

The port is accepted when it passes the ported JS tests. See `docs/PORT-PLAN.md` §4a.

- **Port JS tests 1:1.** Each `Deno.test(name)` becomes a `#[test] fn name_in_snake_case()`,
  with a comment above it citing the JS file and line. Each JS assertion becomes a Rust
  assertion on the same value with the same tolerance. Don't drop, merge, loosen or `#[ignore]`
  anything. If a JS test can't be ported yet because it needs a module from a later milestone,
  mark it in `docs/TEST-MAP.md`. Don't stub it.
- **Add your own thorough tests on top.** For each ported function, cover:
  - the cases JS exercises;
  - edge values: -0, NaN, ±∞, empty input, degenerate or zero-area triangles, exact-threshold
    inputs, points exactly on grid-cell boundaries;
  - every branch, with at least one input reaching it.
- **Parity cases.** Every ported function gets oracle cases recorded from the real JS
  (`docs/PARITY.md`), replayed in a Rust test. Discrete outputs must be equal. Floats must
  be within the tolerance `docs/PARITY.md` gives for that function. `jsnum` and `three` must
  match **bit for bit**.
- A test that fails is information. Report it. Never edit the test or the corpus to make it pass.

## PR body template

```markdown
## Scope
<ticket #, one paragraph>

## JS origin
| Rust item | JS origin (file:lines @ UPSTREAM) |
|---|---|

## Tests
- Ported JS tests: <file → file, N of N cases>
- New Rust tests: <count, what they cover>
- Parity: <cases recorded / replayed / passing, tolerance>
- `just check`: green

## Divergences
<Every place the Rust isn't a literal port of the JS, and why. "None" if none.
 An unlisted divergence found in review fails the review.>

## Open questions
<or "None">
```

## Review and escalation

The orchestrator reads your diff side by side with the JS. Review comments name the file and
line, what JS does, and what to change. If a ticket fails review twice, it is reassigned to
another agent. A third attempt isn't yours to make.
