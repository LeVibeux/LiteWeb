use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Box as GtkBox, Button, Entry, Inhibit, Label, MessageDialog, Notebook, Orientation, ScrolledWindow, Separator};
use webkit2gtk::{LoadEvent, WebView, WebViewExt};

use crate::adblock::Blocker;
use crate::browser::{create_web_context, create_webview, TabManager};
use crate::commands::{CommandAction, CommandPalette};
use crate::energy::{EnergyLevel, EnergyManager};
use crate::storage::Storage;

struct ToolbarParts {
    bar: GtkBox,
    url_entry: Entry,
    eco_label: Label,
    block_label: Label,
    back: Button,
    forward: Button,
    reload: Button,
    bookmark: Button,
    eco: Button,
}

pub struct BrowserWindow {
    window: ApplicationWindow,
    state: Rc<RefCell<AppState>>,
}

struct AppState {
    tabs: TabManager,
    storage: Storage,
    energy: EnergyManager,
    blocker: Rc<Blocker>,
    web_context: webkit2gtk::WebContext,
    notebook: Notebook,
    url_entry: Entry,
    eco_label: Label,
    block_label: Label,
}

impl BrowserWindow {
    pub fn new(app: &Application) -> Self {
        let blocker = Rc::new(Blocker::new());
        let storage = Storage::open();
        let web_context = create_web_context();

        let window = ApplicationWindow::builder()
            .application(app)
            .title("LiteWeb")
            .default_width(1200)
            .default_height(800)
            .build();

        let root = GtkBox::new(Orientation::Vertical, 0);

        let toolbar = Self::build_toolbar();
        let url_entry = toolbar.url_entry.clone();
        let eco_label = toolbar.eco_label.clone();
        let block_label = toolbar.block_label.clone();

        let notebook = Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_show_border(false);

        let content_box = GtkBox::new(Orientation::Vertical, 0);
        content_box.pack_start(&notebook, true, true, 0);

        root.pack_start(&toolbar.bar, false, false, 0);
        root.pack_start(&content_box, true, true, 0);

        window.add(&root);

        let mut tabs = TabManager::new();
        tabs.create_tab("https://duckduckgo.com");

        let state = Rc::new(RefCell::new(AppState {
            tabs,
            storage,
            energy: EnergyManager::new(),
            blocker: blocker.clone(),
            web_context,
            notebook: notebook.clone(),
            url_entry: url_entry.clone(),
            eco_label: eco_label.clone(),
            block_label: block_label.clone(),
        }));

        Self::wire_toolbar(&toolbar, state.clone());
        Self::wire_shortcuts(&window, state.clone());
        Self::wire_notebook(&notebook, state.clone());
        Self::render_tabs(state.clone(), true);
        Self::start_energy_timer(state.clone());

        Self { window, state }
    }

    pub fn show_all(&self) {
        self.window.show_all();
    }

    fn build_toolbar() -> ToolbarParts {
        let bar = GtkBox::new(Orientation::Horizontal, 4);
        bar.set_margin_start(6);
        bar.set_margin_end(6);
        bar.set_margin_top(4);
        bar.set_margin_bottom(4);

        let back = Button::with_label("←");
        back.set_tooltip_text(Some("Retour (Alt+←)"));

        let forward = Button::with_label("→");
        forward.set_tooltip_text(Some("Avant (Alt+→)"));

        let reload = Button::with_label("↻");
        reload.set_tooltip_text(Some("Recharger (F5)"));

        let url_entry = Entry::new();
        url_entry.set_placeholder_text(Some("Adresse ou recherche — Ctrl+L"));
        url_entry.set_hexpand(true);
        url_entry.set_input_purpose(gtk::InputPurpose::Url);

        let bookmark = Button::with_label("★");
        bookmark.set_tooltip_text(Some("Ajouter aux favoris (Ctrl+D)"));

        let eco = Button::with_label("⚡");
        eco.set_tooltip_text(Some("Mode économie (Ctrl+Shift+E)"));

        let eco_label = Label::new(Some("Mode: Normal"));
        let block_label = Label::new(Some("Bloqués: 0"));

        bar.pack_start(&back, false, false, 0);
        bar.pack_start(&forward, false, false, 0);
        bar.pack_start(&reload, false, false, 0);
        bar.pack_start(&url_entry, true, true, 0);
        bar.pack_start(&bookmark, false, false, 0);
        bar.pack_start(&eco, false, false, 0);
        bar.pack_start(&Separator::new(Orientation::Vertical), false, false, 0);
        bar.pack_start(&eco_label, false, false, 0);
        bar.pack_start(&block_label, false, false, 0);

        ToolbarParts {
            bar,
            url_entry,
            eco_label,
            block_label,
            back,
            forward,
            reload,
            bookmark,
            eco,
        }
    }

    fn wire_toolbar(toolbar: &ToolbarParts, state: Rc<RefCell<AppState>>) {
        let state_back = state.clone();
        toolbar.back.connect_clicked(move |_| Self::navigate_back(state_back.clone()));

        let state_fwd = state.clone();
        toolbar
            .forward
            .connect_clicked(move |_| Self::navigate_forward(state_fwd.clone()));

        let state_reload = state.clone();
        toolbar
            .reload
            .connect_clicked(move |_| Self::reload(state_reload.clone()));

        let state_bm = state.clone();
        toolbar
            .bookmark
            .connect_clicked(move |_| Self::bookmark_current(state_bm.clone()));

        let state_eco = state.clone();
        toolbar
            .eco
            .connect_clicked(move |_| Self::toggle_eco(state_eco.clone()));

        let state_submit = state.clone();
        state.borrow().url_entry.connect_activate(move |entry| {
            let text = entry.text().to_string();
            if text.starts_with(':') {
                Self::run_command(state_submit.clone(), &text);
            } else {
                Self::navigate_to(state_submit.clone(), &text);
            }
        });
    }

    fn wire_shortcuts(window: &ApplicationWindow, state: Rc<RefCell<AppState>>) {
        window.connect_key_press_event(move |_, event| {
            let key = event.keyval();
            let mods = event.state();

            let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let alt = mods.contains(gtk::gdk::ModifierType::MOD1_MASK);

            if ctrl && key == gtk::gdk::keys::constants::L {
                let entry = state.borrow().url_entry.clone();
                entry.grab_focus();
                entry.select_region(0, -1);
                return Inhibit(true);
            }
            if ctrl && key == gtk::gdk::keys::constants::T {
                Self::new_tab(state.clone(), "about:blank");
                return Inhibit(true);
            }
            if ctrl && key == gtk::gdk::keys::constants::W {
                Self::close_current_tab(state.clone());
                return Inhibit(true);
            }
            if ctrl && !shift && key == gtk::gdk::keys::constants::Tab {
                let idx = {
                    let mut st = state.borrow_mut();
                    st.tabs.next_tab();
                    st.tabs.active_index()
                };
                Self::switch_to_tab(state.clone(), idx);
                return Inhibit(true);
            }
            if ctrl && shift && key == gtk::gdk::keys::constants::Tab {
                let idx = {
                    let mut st = state.borrow_mut();
                    st.tabs.prev_tab();
                    st.tabs.active_index()
                };
                Self::switch_to_tab(state.clone(), idx);
                return Inhibit(true);
            }
            if ctrl && key == gtk::gdk::keys::constants::R {
                Self::reload(state.clone());
                return Inhibit(true);
            }
            if key == gtk::gdk::keys::constants::F5 {
                Self::reload(state.clone());
                return Inhibit(true);
            }
            if alt && key == gtk::gdk::keys::constants::Left {
                Self::navigate_back(state.clone());
                return Inhibit(true);
            }
            if alt && key == gtk::gdk::keys::constants::Right {
                Self::navigate_forward(state.clone());
                return Inhibit(true);
            }
            if ctrl && key == gtk::gdk::keys::constants::D {
                Self::bookmark_current(state.clone());
                return Inhibit(true);
            }
            if ctrl && shift && key == gtk::gdk::keys::constants::E {
                Self::toggle_eco(state.clone());
                return Inhibit(true);
            }
            if key == gtk::gdk::keys::constants::colon {
                let entry = state.borrow().url_entry.clone();
                entry.set_text(":");
                entry.grab_focus();
                entry.set_position(-1);
                return Inhibit(true);
            }

            Inhibit(false)
        });
    }

    fn wire_notebook(notebook: &Notebook, state: Rc<RefCell<AppState>>) {
        notebook.connect_switch_page(move |_nb, _page, page_num| {
            let index = page_num as usize;
            let state = state.clone();
            glib::idle_add_local_once(move || {
                let needs_render = {
                    let mut st = state.borrow_mut();
                    st.tabs.set_active(index);
                    st.tabs
                        .tabs()
                        .get(index)
                        .map(|t| t.is_suspended())
                        .unwrap_or(false)
                };

                if needs_render {
                    if let Some(tab) = state.borrow_mut().tabs.tabs_mut().get_mut(index) {
                        tab.wake();
                    }
                    Self::render_tabs(state.clone(), true);
                } else if let Some(tab) = state.borrow().tabs.active_tab() {
                    state.borrow().url_entry.set_text(&tab.url);
                }
            });
        });
    }

    fn update_tab_label(state: Rc<RefCell<AppState>>, tab_index: usize) {
        let st = state.borrow();
        let Some(tab) = st.tabs.tabs().get(tab_index) else {
            return;
        };
        let text = if tab.is_suspended() {
            format!("{} 💤", truncate_title(&tab.title))
        } else if tab.modified {
            format!("{} •", truncate_title(&tab.title))
        } else {
            truncate_title(&tab.title)
        };
        if let Some(label) = &tab.tab_label {
            label.set_text(&text);
        }
    }

    fn update_chrome(state: Rc<RefCell<AppState>>) {
        let st = state.borrow();
        if let Some(tab) = st.tabs.active_tab() {
            st.url_entry.set_text(&tab.url);
        }
        st.eco_label
            .set_text(&format!("Mode: {}", st.energy.level().label()));
        st.block_label
            .set_text(&format!("Bloqués: {}", st.blocker.blocked_count()));
    }

    fn switch_to_tab(state: Rc<RefCell<AppState>>, index: usize) {
        state.borrow_mut().tabs.set_active(index);
        let page = state.borrow().notebook.current_page();
        if page != Some(index as u32) {
            state.borrow().notebook.set_current_page(Some(index as u32));
        }
        Self::update_chrome(state);
    }

    fn render_tabs(state: Rc<RefCell<AppState>>, structural: bool) {
        let tab_count = state.borrow().tabs.tabs().len();
        let page_count = state.borrow().notebook.n_pages() as usize;

        if !structural && tab_count == page_count {
            for i in 0..tab_count {
                Self::update_tab_label(state.clone(), i);
            }
            Self::update_chrome(state);
            return;
        }

        if structural {
            let mut st = state.borrow_mut();
            while st.notebook.n_pages() > 0 {
                st.notebook.remove_page(Some(0));
            }
            for tab in st.tabs.tabs_mut().iter_mut() {
                tab.tab_label = None;
            }
        }

        {
            let mut st = state.borrow_mut();
            let web_context = st.web_context.clone();
            let blocker = st.blocker.clone();
            let storage = st.storage.clone();
            let state_for_tabs = state.clone();

            for (index, tab) in st.tabs.tabs_mut().iter_mut().enumerate() {
                if tab.is_suspended() || tab.webview.is_some() {
                    continue;
                }
                let wv = create_webview(&web_context, blocker.clone());
                Self::connect_webview(wv.clone(), index, state_for_tabs.clone(), storage.clone());
                tab.webview = Some(wv);
            }
        }

        while state.borrow().notebook.n_pages() > tab_count as u32 {
            let last = state.borrow().notebook.n_pages() - 1;
            state.borrow().notebook.remove_page(Some(last));
        }

        let (notebook, current_page, url_text, eco_text, block_text) = {
            let mut st = state.borrow_mut();
            let active = st.tabs.active_index();
            let web_context = st.web_context.clone();
            let blocker = st.blocker.clone();
            let storage = st.storage.clone();
            let state_for_tabs = state.clone();

            while (st.notebook.n_pages() as usize) < tab_count {
                let index = st.notebook.n_pages() as usize;
                let tab = &mut st.tabs.tabs_mut()[index];

                let label = Label::new(Some(&truncate_title(&tab.title)));
                label.set_margin_start(6);
                label.set_margin_end(6);
                tab.tab_label = Some(label.clone());

                let page_box = GtkBox::new(Orientation::Vertical, 0);

                if tab.is_suspended() {
                    let info = Label::new(Some(&format!(
                        "Onglet suspendu — {}\nAppuyez pour réactiver",
                        tab.url
                    )));
                    info.set_margin_top(24);
                    page_box.pack_start(&info, true, true, 0);
                } else {
                    if tab.webview.is_none() {
                        let wv = create_webview(&web_context, blocker.clone());
                        Self::connect_webview(
                            wv.clone(),
                            index,
                            state_for_tabs.clone(),
                            storage.clone(),
                        );
                        tab.webview = Some(wv);
                    }

                    if let Some(wv) = &tab.webview {
                        let scrolled = ScrolledWindow::new(
                            None::<&gtk::Adjustment>,
                            None::<&gtk::Adjustment>,
                        );
                        scrolled.add(wv);
                        scrolled.set_hexpand(true);
                        scrolled.set_vexpand(true);
                        page_box.pack_start(&scrolled, true, true, 0);

                        let url = tab.url.clone();
                        if !url.is_empty() && url != "about:blank" {
                            let wv = wv.clone();
                            glib::idle_add_local_once(move || {
                                wv.load_uri(&url);
                            });
                        }
                    }
                }

                st.notebook.append_page(&page_box, Some(&label));
            }

            let current_page = if active < tab_count {
                Some(active as u32)
            } else {
                None
            };

            let url_text = st
                .tabs
                .active_tab()
                .map(|t| t.url.clone())
                .unwrap_or_default();
            let eco_text = format!("Mode: {}", st.energy.level().label());
            let block_text = format!("Bloqués: {}", st.blocker.blocked_count());
            let notebook = st.notebook.clone();

            (notebook, current_page, url_text, eco_text, block_text)
        };

        {
            let st = state.borrow();
            st.url_entry.set_text(&url_text);
            st.eco_label.set_text(&eco_text);
            st.block_label.set_text(&block_text);
        }

        for i in 0..tab_count {
            Self::update_tab_label(state.clone(), i);
        }

        if let Some(page) = current_page {
            notebook.set_current_page(Some(page));
        }
    }

    fn connect_webview(
        wv: WebView,
        tab_index: usize,
        state: Rc<RefCell<AppState>>,
        storage: Storage,
    ) {
        let state_title = state.clone();
        wv.connect_title_notify(move |view| {
            if let Some(title) = view.title() {
                {
                    let mut st = state_title.borrow_mut();
                    if let Some(tab) = st.tabs.tabs_mut().get_mut(tab_index) {
                        tab.title = title.to_string();
                    }
                }
                Self::update_tab_label(state_title.clone(), tab_index);
            }
        });

        let state_load = state.clone();
        wv.connect_load_changed(move |view, event| {
            if event != LoadEvent::Finished {
                return;
            }
            let uri = view.uri();
            let title = view.title();
            let storage = storage.clone();
            let state_load = state_load.clone();
            glib::idle_add_local_once(move || {
                if let Some(uri) = uri {
                    let title = title.unwrap_or_else(|| uri.clone());
                    storage.add_history(&uri, &title);
                    let mut st = state_load.borrow_mut();
                    if let Some(tab) = st.tabs.tabs_mut().get_mut(tab_index) {
                        tab.url = uri.to_string();
                        tab.title = title.to_string();
                        tab.modified = false;
                    }
                    if st.tabs.active_index() == tab_index {
                        st.url_entry.set_text(&uri);
                    }
                    st.block_label
                        .set_text(&format!("Bloqués: {}", st.blocker.blocked_count()));
                }
            });
        });

        let state_click = state.clone();
        wv.connect_button_press_event(move |_, event| {
            if event.button() == 8 {
                Self::navigate_back(state_click.clone());
                return Inhibit(true);
            }
            if event.button() == 9 {
                Self::navigate_forward(state_click.clone());
                return Inhibit(true);
            }
            Inhibit(false)
        });
    }

    fn navigate_to(state: Rc<RefCell<AppState>>, input: &str) {
        let url = CommandPalette::parse(input);
        let url = match url {
            CommandAction::Open(u) => u,
            _ => input.to_string(),
        };

        let mut st = state.borrow_mut();
        let idx = st.tabs.active_index();
        if let Some(tab) = st.tabs.tabs_mut().get_mut(idx) {
            tab.url = url.clone();
            tab.wake();
            if let Some(wv) = &tab.webview {
                wv.load_uri(&url);
            }
        }
        st.url_entry.set_text(&url);
    }

    fn navigate_back(state: Rc<RefCell<AppState>>) {
        if let Some(wv) = state
            .borrow()
            .tabs
            .active_tab()
            .and_then(|t| t.webview.clone())
        {
            if wv.can_go_back() {
                wv.go_back();
            }
        }
    }

    fn navigate_forward(state: Rc<RefCell<AppState>>) {
        if let Some(wv) = state
            .borrow()
            .tabs
            .active_tab()
            .and_then(|t| t.webview.clone())
        {
            if wv.can_go_forward() {
                wv.go_forward();
            }
        }
    }

    fn reload(state: Rc<RefCell<AppState>>) {
        if let Some(wv) = state
            .borrow()
            .tabs
            .active_tab()
            .and_then(|t| t.webview.clone())
        {
            wv.reload();
        }
    }

    fn new_tab(state: Rc<RefCell<AppState>>, url: &str) {
        state.borrow_mut().tabs.create_tab(url);
        Self::render_tabs(state.clone(), true);
    }

    fn close_current_tab(state: Rc<RefCell<AppState>>) {
        let idx = state.borrow().tabs.active_index();
        if state.borrow_mut().tabs.close_tab(idx) {
            if state.borrow().tabs.tabs().is_empty() {
                state.borrow_mut().tabs.create_tab("about:blank");
            }
            Self::render_tabs(state, true);
        }
    }

    fn bookmark_current(state: Rc<RefCell<AppState>>) {
        let st = state.borrow();
        if let Some(tab) = st.tabs.active_tab() {
            st.storage.add_bookmark(&tab.url, &tab.title);
        }
    }

    fn toggle_eco(state: Rc<RefCell<AppState>>) {
        let level = state.borrow_mut().energy.toggle();
        state
            .borrow()
            .eco_label
            .set_text(&format!("Mode: {}", level.label()));
    }

    fn run_command(state: Rc<RefCell<AppState>>, input: &str) {
        let state_cmd = state.clone();
        match CommandPalette::parse(input) {
            CommandAction::Open(url) => Self::navigate_to(state_cmd.clone(), &url),
            CommandAction::Tab(n) => {
                Self::switch_to_tab(state_cmd.clone(), n);
            }
            CommandAction::Suspend => {
                let idx = state_cmd.borrow().tabs.active_index();
                state_cmd.borrow_mut().tabs.suspend_tab(idx);
                Self::render_tabs(state_cmd.clone(), true);
            }
            CommandAction::SuspendAll => {
                state_cmd.borrow_mut().tabs.suspend_all_except_active();
                Self::render_tabs(state_cmd.clone(), true);
            }
            CommandAction::EcoOn => {
                state_cmd.borrow_mut().energy.set_level(EnergyLevel::Eco);
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::EcoOff => {
                state_cmd.borrow_mut().energy.set_level(EnergyLevel::Normal);
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::EcoAggressive => {
                state_cmd
                    .borrow_mut()
                    .energy
                    .set_level(EnergyLevel::Aggressive);
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::BookmarkAdd => Self::bookmark_current(state_cmd.clone()),
            CommandAction::BookmarkList => Self::show_bookmarks(state_cmd.clone()),
            CommandAction::History => Self::show_history(state_cmd.clone()),
            CommandAction::DownloadList => {
                Self::show_message("Téléchargements", "Gestion des téléchargements — bientôt disponible.")
            }
            CommandAction::Unknown(cmd) => {
                Self::show_message("Commande inconnue", &format!("'{cmd}' n'est pas reconnue."));
            }
        }
        state.borrow().url_entry.set_text("");
    }

    fn show_bookmarks(state: Rc<RefCell<AppState>>) {
        let bookmarks = state.borrow().storage.list_bookmarks();
        let text = if bookmarks.is_empty() {
            "Aucun favori.".to_string()
        } else {
            bookmarks
                .iter()
                .map(|b| format!("• {} — {}", b.title, b.url))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Self::show_message("Favoris", &text);
    }

    fn show_history(state: Rc<RefCell<AppState>>) {
        let entries = state.borrow().storage.recent_history(20);
        let text = if entries.is_empty() {
            "Historique vide.".to_string()
        } else {
            entries
                .iter()
                .map(|e| format!("• {} — {}", e.title, e.url))
                .collect::<Vec<_>>()
                .join("\n")
        };
        Self::show_message("Historique", &text);
    }

    fn show_message(title: &str, body: &str) {
        let dialog = MessageDialog::new(
            None::<&gtk::Window>,
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Info,
            gtk::ButtonsType::Ok,
            body,
        );
        dialog.set_title(title);
        dialog.connect_response(|d, _| d.close());
        dialog.show_all();
    }

    fn start_energy_timer(state: Rc<RefCell<AppState>>) {
        glib::timeout_add_local(Duration::from_secs(30), move || {
            let timeout = state.borrow().energy.level().suspend_timeout();
            let now = std::time::Instant::now();
            let indices = state.borrow().tabs.inactive_indices(now, timeout);
            if !indices.is_empty() {
                for idx in indices {
                    state.borrow_mut().tabs.suspend_tab(idx);
                }
                Self::render_tabs(state.clone(), true);
            }

            let max = state.borrow().energy.level().max_active_tabs();
            let active_count = state.borrow().tabs.count_active_webviews();
            if active_count > max {
                let to_suspend = active_count - max;
                let mut suspended = 0usize;
                let active_idx = state.borrow().tabs.active_index();
                let len = state.borrow().tabs.tabs().len();
                for i in 0..len {
                    if i == active_idx {
                        continue;
                    }
                    if suspended >= to_suspend {
                        break;
                    }
                    let should = state
                        .borrow()
                        .tabs
                        .tabs()
                        .get(i)
                        .map(|t| !t.is_suspended())
                        .unwrap_or(false);
                    if should {
                        state.borrow_mut().tabs.suspend_tab(i);
                        suspended += 1;
                    }
                }
                Self::render_tabs(state.clone(), true);
            }

            glib::Continue(true)
        });
    }
}

fn truncate_title(title: &str) -> String {
    if title.chars().count() > 24 {
        format!("{}…", title.chars().take(22).collect::<String>())
    } else {
        title.to_string()
    }
}
