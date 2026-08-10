# Design: commandes `:tab new|next|prev`

Date: 2026-08-10  
Branche: `feat/keyboard-functions`

## Goal

Exposer la gestion d’onglets via la barre de commandes, en plus des raccourcis clavier existants (Ctrl+T, Ctrl+Tab, Ctrl+Shift+Tab). Conserver le saut par numéro `:tab N`.

## Command surface

| Input | Action |
|---|---|
| `:tab new` | Nouvel onglet sur `about:blank` |
| `:tab new <url-or-query>` | Nouvel onglet ; URL normalisée comme `:open` |
| `:tab next` | Onglet suivant (boucle) |
| `:tab prev` | Onglet précédent (boucle) |
| `:tab N` / `:t N` | Saut vers l’onglet N (1-indexé) — inchangé |

L’alias `:t` accepte les mêmes sous-formes (`:t new`, `:t next`, `:t prev`, `:t 2`).

Ctrl+T reste inchangé et crée un onglet `about:blank`.

## Approach

Sous-commandes de `:tab` (même famille que `:eco on|off`), pas de commandes plates séparées ni d’alias courts supplémentaires (`:tn`, etc.).

## Architecture

### `CommandAction` (`src/commands/palette.rs`)

Nouveaux variants :

- `TabNew(String)` — URL déjà normalisée (ou `about:blank`)
- `TabNext`
- `TabPrev`

`Tab(usize)` inchangé.

### Parsing

Pour `verb` `tab` | `t` :

1. Si `arg` commence par `new` (mot entier) → `TabNew` avec le reste normalisé via `normalize_url` ; reste vide → `about:blank`
2. Si `arg` == `next` → `TabNext`
3. Si `arg` == `prev` → `TabPrev`
4. Si `arg` parse en `usize` → `Tab(n.saturating_sub(1))`
5. Sinon → `Unknown`

Comparaisons de sous-commandes en minuscules.

### Wiring UI (`src/ui/window.rs`)

Dans `run_command` :

- `TabNew(url)` → `Self::new_tab(state, &url)` (déjà utilisé par Ctrl+T)
- `TabNext` → `tabs.next_tab()` puis `switch_to_tab` (même chemin que Ctrl+Tab)
- `TabPrev` → `tabs.prev_tab()` puis `switch_to_tab` (même chemin que Ctrl+Shift+Tab)

### Hints / placeholder

Mettre à jour :

- `KEYBAR_COMMAND` pour mentionner `:tab new|next|prev|N`
- placeholder de la command entry (ex. `open example.com  |  tab new [url]  |  tab next`)

## Error handling

- Sous-commande `tab` inconnue → dialogue « Commande inconnue » via `Unknown` (comportement existant)
- `:tab` sans argument → `Unknown` (pas de comportement implicite)

## Testing

Tests unitaires dans `palette.rs` :

- `:tab new` → `TabNew("about:blank")`
- `:tab new example.com` → `TabNew("https://example.com")`
- `:tab next` / `:tab prev` → variants correspondants
- `:t new` → même que `:tab new`
- `:tab 2` → `Tab(1)` (régression)

## Out of scope

- Fermeture d’onglet via commande (`:tab close`)
- Alias courts (`:tn`, `:tp`)
- Changement du comportement de Ctrl+T
