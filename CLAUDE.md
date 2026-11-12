# CLAUDE.md

Code style for this repo. Apply it without being asked. Where neighbouring code disagrees with this file, follow this file.

## Conformance check

Lint only catches what lint can see. The rules that need judgement (comments, naming, single purpose, duplication, hierarchy, props placement) have drifted in other repos while lint stayed clean. At the end of any task that touches more than one file or adds a file, before reporting it done:

1. Run every command in [Before calling a change done](#before-calling-a-change-done). All must pass.
2. Spawn a subagent on the lightweight model (`Agent` tool with `model: "haiku"`). Give it this file and the full change set (`git diff` plus every untracked file). Have it report every violation, grouped by rule, with `file:line` and a one-line quote. Comments first, since that is where drift has been worst.
3. Fix everything it reports, rerun step 1, and list any judgement calls in your response. A missed rule is a miss, so don't describe it as a judgement call.

## All languages

### Comments

- Default to none. This includes file headers, `//!` and `///` in Rust, and JSDoc (`/** */`) in TypeScript. Doc comments on every constant or item is a habit to override.
- Before writing a comment, ask whether the code could say it, or whether a better name could. If so, delete the comment or rename.
- The one exception is a fact about something outside the code that the next reader would otherwise get wrong: a library bug, a platform rule, a protocol id, a third-party coordinate convention. It gets one line, in `//` style, in ordinary language.
- Never write a comment that records how you worked something out, justifies a change, or says where a value came from (a Figma frame, a disassembly, the request). That goes in your reply.
- Never write `TODO` or `FIXME`.
- Long rationale goes in `README.md`.
- Judge each comment by its content. How many comments other files have is irrelevant.

### Naming

- Names are fully descriptive, with no uncommon abbreviations: `index` not `idx`, `context` not `ctx`, `event` not `e`, `request` not `req`, `leftWristLandmark` not `lwLm`. Loop counters (`i`) and abbreviations standard in the domain (`url`, `id`, `rgb`, `fps`) are fine.
- Booleans start with `is` or `has`, falling back to `should`, `can` or `will`, in every layer. A settings flag is `has_fish_eye_correction` in Rust and `hasFishEyeCorrection` in JSON and TypeScript, never `fisheyeCorrection`. Bare words like `connected` or `loaded` are not boolean names.
- File and folder names say what they hold without being opened: `dottedPath.ts` not `paths.ts`, `AutoScrollLogPane` not `LogFeed`.

### Functions and files

- A function does one thing. If you can't name it without "and", split it.
- Pure functions are the default: take inputs, return a derived value, touch nothing else. Keep side effects at the edges.
- Don't mutate arguments. Return a new value. The exception is a hot per-frame path where allocation cost matters.
- Prefer early returns over nesting.
- Each file has one job. A helper with one consumer stays next to it, and rises to the closest common ancestor only when a second consumer appears. Never reach sideways into another feature's folder.
- Model "nothing here" with `null` or `Option`, never a sentinel value such as `-1` or `64`.

### Literals and constants

- A value with one use stays inline where it is used. Moving it away makes it harder to read and tune.
- Name it as a constant when several places share it, when it is fixed by something external (a protocol, file format or hardware limit), or when the bare number would otherwise need a comment.

### Design

- Before adding a wrapper, parameter or defensive pattern, ask what breaks if the pieces are merged. A hook consumed by exactly one other hook is a smell, as is complexity that only protects the abstraction itself. Extract shared code only when a second consumer actually exists.
- When there is a proper fix and a cheaper mitigation, build the proper fix, kept tight. Don't offer the mitigation as an alternative.
- Don't invent behaviour the request didn't ask for. If a requested setting has no obvious use, ask what it should drive. Any structural addition that wasn't agreed (a new build output, a new package shape, a new public API) is a proposal to raise before building it.
- When porting from another system, read the original to learn what it does, then write it the way this codebase would. Don't copy its control flow or its integer tricks.

### Dependencies

Before running `npm install` or `cargo add`, check `package.json` or `Cargo.toml` for a library that already covers the need. Only propose a new dependency when the existing ones can't do the job, and say so.

Install with the package manager so you get the latest version:

```bash
npm install <package>                                  # in ./
cargo add <crate> --manifest-path backend/Cargo.toml
```

Never hand-write a version unless there is a concrete reason to pin (an upstream bug, an API break, a peer constraint). State the reason in your response.

### Suppressions

Fix the cause. If a lint must be suppressed, give a one-line reason and always call it out in your response.

```ts
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- external lib returns untyped payload
```

## TypeScript

Use current ESNext as the language is written today. Common patterns from older code are not evidence of what's right here, so if something feels standard, check `./tsconfig.json` before relying on it.

- ES modules only. Never `require` or `module.exports`.
- Every import names a file, never a folder. Never create an `index.ts` that re-exports a folder's contents (a "barrel file").
- Omit the `.ts` and `.tsx` extension on imports. Keep the extension on anything the bundler resolves by extension, such as `.css` and `.json`.
- Use `import type` for types.
- Use `const` by default and `let` only when reassigned. Never `var`.
- Arrow functions only. Never the `function` keyword.
- Use `type`, not `interface`. The only exception is declaration merging, such as augmenting `Window` in a `.d.ts` file.
- Let TypeScript infer return types. Don't annotate them unless inference fails.
- Always use braces on `if`, `else`, `for` and `while`, even for one statement.
- Use the language before a helper or library: `?.` and `??` over manual guards, plus `at()`, `toSorted()`, `structuredClone`, `Object.groupBy`, `Promise.withResolvers` and `using` where they read cleanly and the tsconfig `lib` includes them.
- Prefer `map`, `filter` and `reduce` over loops when producing a derived value. Loops are fine for per-frame side effects.
- Function names lead with an action verb: `get`, `set`, `create`, `build`, `parse`, `update`, `calculate`. No bare nouns (`position`, `data`) and no vague verbs (`do`, or `handle` outside an event handler).
- Catch bindings are `caughtError`, never `e` or `err`.
- Write object keys, type members, union members, JSX props and imports in alphabetical order as you go. `eslint-plugin-perfectionist`, an ESLint plugin that sorts these and treats disorder as an error, enforces this.
- Delete unused `.ts` and `.tsx` files. Never delete a `.d.ts` file on the grounds that nothing imports it, because it may declare ambient types.

## React

### Components

- One component per file. It is a default export, declared as an arrow function, with the export at the bottom.
- A `ComponentNameProps` type sits directly above the component, even for one prop. No anonymous prop types (`({ x }: { x: number })`).

  ```tsx
  type CounterProps = { value: number }

  const Counter = ({ value }: CounterProps) => <span>{value}</span>

  export default Counter
  ```

- Don't export a props type that nothing imports. A props type shared across components is named with a `Shared` prefix (`SharedScreenProps`) and lives in a shared types file.
- Optional props use `?:` in the type. Never `defaultProps`.
- Component names are PascalCase with at least one lowercase letter (`Hud`, not `HUD`), because react-refresh, the hot-reload tool, treats all-caps identifiers as constants.
- A `.tsx` file exports a component and nothing else. Small helpers for that component stay in the file. Substantial ones go in a sibling `.ts` file (`Field/fieldUtils.ts`), never another `.tsx`.
- Split a component when it is hard to take in at a glance or mixes concerns such as data wrangling, rendering and side effects. Either split out child components or move the logic to a sibling `.ts` file.

### Events, hooks and state

- Event handlers start with `handle` (`handleClick`). Props that pass them through start with `on` (`onClick`).
- A hook owns its whole job. Prefer one hook that subscribes and returns the value over a callback-taking hook wrapped by a second hook.
- Keep state local with `useState` or `useRef`, and pass shared refs through the parent. Promote state to a global store only when it is truly cross-cutting.
- The global store in the other repos is `zustand`. Subscribe with selector functions, not whole-store destructuring. It isn't installed here, so ask before adding it.

### File system hierarchy

The directory tree shows component ownership.

- Each component lives in a folder named after it: `Foo/Foo.tsx`, with styles at `Foo/Foo.module.css`.
- A folder holds exactly one component, plus its styles and utilities. A second component gets its own folder, as a child if the first owns it and as a sibling if it stands alone.
- A component rendered by one parent goes in a subfolder of that parent. A shared component sits at the deepest folder that is an ancestor of every consumer, never higher.

```
src/
  Meeting/             # Meeting and everything it owns
    MeetingControls/
  Agenda/
    BookingRow/
  components/          # used by more than one top-level feature
  hooks/               # app-wide custom hooks
  types/               # shared type definitions
```

## Rust

- Write idiomatic, human-readable Rust. Name functions for what they do. Unlike TypeScript, there's no need for a `get_` prefix.
- Keep the core pure and push IO (network, processes, threads, stdout) to the edges. One concern per module, one concern per file within a module.
- `main.rs` is a thin entry point with no domain logic.
- Model absence with `Option` and failure with `Result`.
- Never use `unwrap` or `expect` on runtime IO or anything derived from external input. No panics outside tests. Reserve `expect` for true invariants. `unwrap` is fine in tests.
- Use `anyhow::Result` with `.context(...)` or `.with_context(...)` saying what failed, and `bail!` for invalid data. Use a hand-written error enum only when a caller branches on the variant.
- Pass ownership over channels and let the type system enforce thread boundaries.
- Borrow across function boundaries (`&str`, `&[u8]`) and return owned values only when ownership transfers. Don't clone needlessly.
- Prefer iterators and combinators over manual loops where that reads clearly. Index and `while` loops are fine for byte-stream cursors, where advancing the position is the point.
- Use a small struct instead of a tuple once a tuple has more than two meaningful fields.
- `rustfmt` owns layout. Don't hand-format.
- If a Clippy lint must be suppressed, use `#[expect(...)]` with a one-line comment saying why, so the build fails once the suppression is no longer needed. Use `#[allow(...)]` only where the lint may or may not fire. Always call out either in your response.

## Formatting

Prettier owns frontend layout (`npm run format` in `./`), with no semicolons, single quotes, trailing commas and 120 columns. Don't hand-format, and don't re-add stylistic ESLint rules.

## Before calling a change done

In `backend/`, all must pass clean:

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

In `./`:

```bash
npm run check    # runs tsc --noEmit, ESLint, then the Prettier check
npm run build
npm test
```

If a check fails, fix the cause rather than suppressing it.

## Notes on this repo

- ESLint is held at version 9 because `eslint-plugin-react` doesn't support version 10 yet. Remove the hold once it does.
- `verbatimModuleSyntax` is off in `./tsconfig.json`, so `import type` is not enforced, but use it anyway.
- `backend/src/providers/google_meet.js`, `backend/src/providers/teams/teams_automation.js`, `backend/src/mock_meet.html` and `backend/src/mock_teams.html` follow the TypeScript rules where they apply, but neither ESLint nor Prettier checks them.
