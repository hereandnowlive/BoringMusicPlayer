# CLAUDE.md

Guidance for Claude when working in this repository.

## Codebase guide

The map of the code, the rules it depends on, and step-by-step playbooks for common changes are in skills.md, imported here so it loads every session:

@skills.md

## About the developer

I am a solo developer and this is a personal project. I know C# and TypeScript well, but I am new to Rust.

## 1. Document Rust extensively

Treat every piece of Rust code as a learning opportunity for me.

- Give every function, struct, enum, trait, and module a doc comment (`///` or `//!`) explaining what it does and why it exists.
- Add inline comments wherever the code uses something a C#/TypeScript developer would not recognise at a glance: ownership and borrowing, lifetimes, `Result`/`Option` and the `?` operator, pattern matching, traits and generics, macros, `Arc`/`Mutex`, async, and so on.
- Where it helps, compare the concept to its C# or TypeScript equivalent.
- Add a link when it is worth it, for example when a concept is new or subtle. Prefer official sources:
  - The Rust Book: https://doc.rust-lang.org/book/
  - Rust by Example: https://doc.rust-lang.org/rust-by-example/
  - Standard library docs: https://doc.rust-lang.org/std/
  - Crate docs: https://docs.rs/
  - Tauri docs: https://tauri.app/
- Do not link for the sake of it. Link once per concept per file, not on every occurrence.
- When explaining Rust in chat, explain the reasoning as well as the fix, especially for compiler errors.

## 2. List all changed files at the end of every action

After any action that changes files, end the reply with a list of every file touched so I can review the changes.

- Group them as created, modified, and deleted.
- Use clickable relative paths, with a short note on what changed in each.
- Include files changed indirectly, such as lock files or generated files.
- If nothing changed, say so.

## 3. Question my design

Because I work alone, nobody else reviews my design decisions. If you see a problem with my design, or know of a better alternative, tell me.

- Ask for clarification instead of guessing when a request is ambiguous or seems to conflict with the existing code.
- Raise concerns before implementing, not after. Briefly explain the problem, the alternative, and the trade-off, then let me decide.
- There are no team conventions or backwards-compatibility constraints to protect, so suggesting a larger change is fine when it is clearly better.
- Once I have decided, go with my decision.
