# Design: fermeture d’onglet (graphique + commande)

Date: 2026-09-14  
Statut: backlog (pas encore implémenté)

## Goal

Fermer un onglet sans le clavier uniquement : bouton × sur l’onglet, et `:tab close` dans la palette. Réutiliser le chemin déjà utilisé par Ctrl+W.

## État actuel

| Surface | Comportement |
|---|---|
| Ctrl+W | `BrowserWindow::close_current_tab` → `TabManager::close_tab(active)` |
| Dernier onglet | recrée `about:blank`, puis `render_tabs(..., true)` |
| Tab header | `gtk::Label` seul (titre, 💤, •) — pas de bouton |
| Palette | `:tab new\|next\|prev\|N` — pas de `close` |

## Command surface

| Input | Action |
|---|---|
| `:tab close` / `:t close` | Ferme l’onglet actif (même sémantique que Ctrl+W) |
| `:tab close N` / `:t close N` | Ferme l’onglet N (1-indexé), même convention que `:tab N` |

Ctrl+W inchangé.

## Approach

Un seul helper UI `close_tab_at(state, index)` :

1. `tabs.close_tab(index)`
2. Si plus d’onglets → `create_tab("about:blank")`
3. `render_tabs(state, true)`

Ctrl+W, `:tab close`, et le bouton × appellent ce helper. Pas de duplication de la logique « dernier onglet ».

## Architecture

### `TabManager::close_tab`

Aujourd’hui, si on ferme un onglet **avant** l’actif, `active` n’est pas décrémenté (ex. onglets `[A,B,C]`, actif `B`, fermer `A` → l’actif devient `C`).

Corriger :

- si `index < active` → `active -= 1`
- si `index == active` et `active == len` après remove → `active = len - 1` (déjà là)
- si `index > active` → `active` inchangé

Tests unitaires : fermer un onglet avant / égal / après l’actif ; fermer le dernier ; `index` hors bornes → `false`.

### `CommandAction` (`src/commands/palette.rs`)

Nouveau variant : `TabClose(Option<usize>)`

- `None` = onglet actif
- `Some(n)` = index 0-based déjà saturé (`N.saturating_sub(1)`), comme `Tab(usize)`

Parsing (`tab` | `t`), mot `close` :

1. Pas d’argument après `close` → `TabClose(None)`
2. Reste parseable en `usize` → `TabClose(Some(n.saturating_sub(1)))`
3. Sinon → `Unknown`

### Wiring UI (`src/ui/window.rs`)

`run_command` :

- `TabClose(None)` → `close_tab_at(state, tabs.active_index())`
- `TabClose(Some(n))` → no-op silencieux si `n` hors bornes (ou message « Onglet inexistant ») ; sinon `close_tab_at`

Hints : `KEYBAR_COMMAND` et placeholder mentionnent `:tab close`.

### Graphique — header d’onglet

Dans `render_tabs`, le widget d’onglet n’est plus le `Label` nu :

```
GtkBox (horizontal)
  ├─ Label  (titre — toujours `tab.tab_label`)
  └─ Button (×, `relief=None`, `focus_on_click=false`)
```

- Le callback du bouton capture `tab.id` (pas l’index). Au clic : `index_of_id` puis `close_tab_at`.
- Ne pas `wake()` : fermer un onglet suspendu le jette, on ne le restaure pas.
- Rebuild structurel (`render_tabs(..., true)`) déjà requis par Ctrl+W ; le bouton suit le même chemin. `notebook_sync_depth` reste en vigueur pendant le rebuild.
- `update_tab_label` continue de n’écrire que le `Label` titre.

## Error handling

- `:tab close` sans onglets impossibles en pratique (toujours au moins `about:blank` après close).
- `:tab close abc` → `Unknown` (dialogue existant).
- `:tab close 99` hors bornes → pas de panic ; hint ou dialogue court.

## Testing

Parser (`palette.rs`) :

- `:tab close` → `TabClose(None)`
- `:t close` → idem
- `:tab close 2` → `TabClose(Some(1))`
- `:tab close foo` → `Unknown`
- régression `:tab new|next|prev|2`

`TabManager` :

- fermer un index `< active` conserve le bon onglet actif
- fermer l’actif bascule vers un voisin (dernier → précédent)
- fermer le dernier onglet laisse `tabs` vide (`true`) — l’UI recrée `about:blank`

## Out of scope

- Clic milieu pour fermer
- Alias courts (`:tc`)
- Dialogue de confirmation
- Fermer tous / fermer les autres
- Changer Ctrl+W
