# Tab Commands (`:tab new|next|prev`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `:tab new [url]`, `:tab next`, and `:tab prev` command-palette actions while keeping `:tab N` and Ctrl+T unchanged.

**Architecture:** Extend `CommandAction` and `CommandPalette::parse` with tab subcommands; wire them in `BrowserWindow::run_command` to existing `new_tab` / `next_tab` / `prev_tab` helpers; update command-bar hints.

**Tech Stack:** Rust, GTK3, WebKitGTK, existing LiteWeb command palette.

**Spec:** `docs/superpowers/specs/2026-08-10-tab-commands-design.md`

## Global Constraints

- Alias `:t` must accept the same sub-forms as `:tab`
- `:tab new` without URL → `about:blank`; with URL → normalize like `:open`
- `:tab` with empty/unknown arg → `Unknown` (not silent `Tab(0)`)
- No `:tab close`, no short aliases (`:tn`), no Ctrl+T behavior change
- Reuse existing UI helpers; do not duplicate tab-switch logic

## File map

| File | Role |
|------|------|
| `src/commands/palette.rs` | Parse + `CommandAction` variants + unit tests |
| `src/ui/window.rs` | `run_command` wiring + hints/placeholder |

---

### Task 1: Parser — `TabNew` / `TabNext` / `TabPrev`

**Files:**
- Modify: `src/commands/palette.rs`
- Test: same file (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `CommandAction::TabNew(String)`, `TabNext`, `TabPrev`; `parse(":tab …")` returns these
- Consumes: existing `normalize_url`

- [ ] **Step 1: Write failing tests**

Add to `palette.rs` tests:

```rust
#[test]
fn parses_tab_new_blank() {
    assert_eq!(
        CommandPalette::parse(":tab new"),
        CommandAction::TabNew("about:blank".into())
    );
}

#[test]
fn parses_tab_new_url() {
    assert_eq!(
        CommandPalette::parse(":tab new example.com"),
        CommandAction::TabNew("https://example.com".into())
    );
}

#[test]
fn parses_tab_next_prev() {
    assert_eq!(CommandPalette::parse(":tab next"), CommandAction::TabNext);
    assert_eq!(CommandPalette::parse(":tab prev"), CommandAction::TabPrev);
}

#[test]
fn parses_t_alias_for_tab_new() {
    assert_eq!(
        CommandPalette::parse(":t new"),
        CommandAction::TabNew("about:blank".into())
    );
}

#[test]
fn parses_tab_index_still_works() {
    assert_eq!(CommandPalette::parse(":tab 2"), CommandAction::Tab(1));
}

#[test]
fn tab_empty_is_unknown() {
    match CommandPalette::parse(":tab") {
        CommandAction::Unknown(_) => {}
        other => panic!("expected Unknown, got {other:?}"),
    }
}
```

- [ ] **Step 2: Run tests — expect fail**

Run: `cargo test --lib parses_tab_new_blank parses_tab_next_prev -- --nocapture`  
(or `cargo test -- commands::palette` if lib target differs)

Expected: compile error / fail because variants missing.

- [ ] **Step 3: Minimal implementation**

1. Add to `CommandAction`:

```rust
TabNew(String),
TabNext,
TabPrev,
```

2. Replace the `"tab" | "t"` arm with:

```rust
"tab" | "t" => {
    let arg_l = arg.to_lowercase();
    let mut words = arg.split_whitespace();
    let first = words.next().unwrap_or("").to_lowercase();
    match first.as_str() {
        "" => CommandAction::Unknown(cmd.to_string()),
        "new" => {
            let rest: String = words.collect::<Vec<_>>().join(" ");
            let url = if rest.is_empty() {
                "about:blank".to_string()
            } else {
                Self::normalize_url(&rest)
            };
            CommandAction::TabNew(url)
        }
        "next" => CommandAction::TabNext,
        "prev" => CommandAction::TabPrev,
        _ => {
            if let Ok(n) = first.parse::<usize>() {
                CommandAction::Tab(n.saturating_sub(1))
            } else {
                CommandAction::Unknown(cmd.to_string())
            }
        }
    }
}
```

Remove unused `arg_l` if not needed — use only `first` / `words` as above.

- [ ] **Step 4: Run tests — expect pass**

Run: `cargo test -- commands::palette`

Expected: all palette tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/commands/palette.rs
git commit -m "feat: parse :tab new|next|prev subcommands"
```

---

### Task 2: Wire UI + hints

**Files:**
- Modify: `src/ui/window.rs` (`KEYBAR_COMMAND`, placeholder, `run_command`)

**Interfaces:**
- Consumes: `CommandAction::TabNew(String)`, `TabNext`, `TabPrev` from Task 1
- Uses: `Self::new_tab`, `st.tabs.next_tab` / `prev_tab`, `Self::switch_to_tab`

- [ ] **Step 1: Update hints and placeholder**

`KEYBAR_COMMAND`:

```rust
const KEYBAR_COMMAND: &str =
    ":open :tab new|next|prev|N :suspend :suspend-all :eco on|off|aggressive :bookmark :history  |  Entrée → exécuter  |  Échap → annuler";
```

Placeholder on command entry:

```rust
command_entry.set_placeholder_text(Some("open example.com  |  tab new [url]  |  tab next"));
```

- [ ] **Step 2: Wire `run_command`**

After the existing `CommandAction::Tab(n)` arm, add:

```rust
CommandAction::TabNew(url) => Self::new_tab(state_cmd.clone(), &url),
CommandAction::TabNext => {
    let idx = {
        let mut st = state_cmd.borrow_mut();
        st.tabs.next_tab();
        st.tabs.active_index()
    };
    Self::switch_to_tab(state_cmd.clone(), idx);
}
CommandAction::TabPrev => {
    let idx = {
        let mut st = state_cmd.borrow_mut();
        st.tabs.prev_tab();
        st.tabs.active_index()
    };
    Self::switch_to_tab(state_cmd.clone(), idx);
}
```

Match the Ctrl+Tab / Ctrl+Shift+Tab pattern already in `wire_shortcuts`.

- [ ] **Step 3: Build check**

Run: `cargo build`  
Expected: success (exhaustive match on `CommandAction`).

- [ ] **Step 4: Commit**

```bash
git add src/ui/window.rs
git commit -m "feat: wire :tab new|next|prev in command bar"
```

---

## Spec coverage check

| Spec item | Task |
|-----------|------|
| `:tab new` / `:tab new url` | 1 + 2 |
| `:tab next` / `:tab prev` | 1 + 2 |
| `:tab N` unchanged | 1 |
| `:t` alias | 1 |
| Ctrl+T unchanged | (no change) |
| Hints / placeholder | 2 |
| Empty/unknown → Unknown | 1 |
| Tests | 1 |
