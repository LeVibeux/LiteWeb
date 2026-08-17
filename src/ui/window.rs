use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gio::Cancellable;
use gdk_pixbuf::prelude::*;
use gdk_pixbuf::{InterpType, Pixbuf, PixbufLoader};
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Entry, Image, Inhibit, Label,
    MessageDialog, Notebook, Orientation, Separator,
};
use pango::EllipsizeMode;
use webkit2gtk::{LoadEvent, UserContentManager, WebResourceExt, WebView, WebViewExt};

const KEYBAR_NORMAL: &str =
    "Ctrl+L URL  |  : commandes  |  Ctrl+T onglet  |  Ctrl+W fermer  |  Ctrl+Tab suivant  |  Alt+←/→ nav  |  F5 recharger  |  Ctrl+D favori  |  Ctrl+Shift+E éco";
const KEYBAR_URL: &str =
    "Entrée → naviguer  |  Ctrl+L focus  |  : → barre commande  |  Échap → annuler";
const KEYBAR_COMMAND: &str =
    ":open :tab new|next|prev|N :suspend :suspend-all :eco on|off|aggressive|ultra :bookmark :history  |  Entrée → exécuter  |  Échap → annuler";
const LOGO_BYTES: &[u8] = include_bytes!("../../assets/liteweb-logo-grok.jpg");

use crate::adblock::Blocker;
use crate::benchmark::{
    BenchmarkConfig, BenchmarkReporter, BenchmarkScenario, IDLE_MEASUREMENT_SECS,
    POST_SUSPENSION_SECS, WARMUP_SECS,
};
use crate::browser::{
    apply_webview_policy, create_user_content_manager, create_web_context, create_webview,
    flatten_html, set_archaic_stylesheet, TabManager,
};
use crate::commands::{is_safe_navigation_url, CommandAction, CommandPalette};
use crate::energy::{EnergyLevel, EnergyManager};
use crate::storage::Storage;

struct CommandBarParts {
    bar: GtkBox,
    command_entry: Entry,
    hints_label: Label,
}

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
    user_content: UserContentManager,
    notebook: Notebook,
    url_entry: Entry,
    command_entry: Entry,
    eco_label: Label,
    block_label: Label,
    hints_label: Label,
    benchmark: Option<BenchmarkRun>,
    /// Depth of programmatic Notebook mutations. While > 0, ignore switch-page
    /// side effects (rebuilds and set_current_page emit switch-page; handling
    /// those would touch last_active / wake suspended tabs).
    notebook_sync_depth: u32,
}

struct BenchmarkRun {
    reporter: BenchmarkReporter,
    warmup_reported: bool,
    first_suspension_at: Option<Instant>,
    all_suspended_at: Option<Instant>,
}

impl BrowserWindow {
    pub fn new(app: &Application, benchmark: Option<BenchmarkConfig>) -> Self {
        let blocker = Rc::new(Blocker::new());
        let storage = Storage::open();
        let web_context = create_web_context();
        let user_content = create_user_content_manager();

        let window = ApplicationWindow::builder()
            .application(app)
            .title("LiteWeb")
            .default_width(1200)
            .default_height(800)
            .build();
        if let Some(logo) = Self::logo_pixbuf(64) {
            window.set_icon(Some(&logo));
        }

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

        let command_bar = Self::build_command_bar();

        root.pack_start(&toolbar.bar, false, false, 0);
        root.pack_start(&content_box, true, true, 0);
        root.pack_start(&command_bar.bar, false, false, 0);

        window.add(&root);

        let mut tabs = TabManager::new();
        let initial_urls = benchmark
            .as_ref()
            .map(BenchmarkConfig::initial_urls)
            .unwrap_or_else(|| vec!["https://duckduckgo.com".to_string()]);
        for url in initial_urls {
            tabs.create_tab(url);
        }

        let mut energy = EnergyManager::new();
        if let Some(config) = &benchmark {
            match config.scenario {
                BenchmarkScenario::Aggressive => energy.set_level(EnergyLevel::Aggressive),
                BenchmarkScenario::Ultra => energy.set_level(EnergyLevel::Ultra),
                BenchmarkScenario::Idle | BenchmarkScenario::Normal | BenchmarkScenario::Loaded => {}
            }
        }
        set_archaic_stylesheet(
            &user_content,
            energy.level().webview_policy().archaic_stylesheet,
        );
        let benchmark_run = benchmark.map(|config| BenchmarkRun {
            reporter: BenchmarkReporter::new(config),
            warmup_reported: false,
            first_suspension_at: None,
            all_suspended_at: None,
        });

        let state = Rc::new(RefCell::new(AppState {
            tabs,
            storage,
            energy,
            blocker: blocker.clone(),
            web_context,
            user_content,
            notebook: notebook.clone(),
            url_entry: url_entry.clone(),
            command_entry: command_bar.command_entry.clone(),
            eco_label: eco_label.clone(),
            block_label: block_label.clone(),
            hints_label: command_bar.hints_label.clone(),
            benchmark: benchmark_run,
            notebook_sync_depth: 0,
        }));

        Self::wire_toolbar(&toolbar, state.clone());
        Self::wire_command_bar(&command_bar, state.clone());
        Self::wire_shortcuts(&window, state.clone());
        Self::wire_notebook(&notebook, state.clone());
        Self::render_tabs(state.clone(), true);
        Self::activate_benchmark_sentinel(state.clone());
        Self::start_energy_timer(state.clone());
        Self::start_benchmark_timer(state.clone(), app.clone());

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

        let logo = Image::from_pixbuf(Self::logo_pixbuf(28).as_ref());
        logo.set_tooltip_text(Some("Logo généré avec Grok Image"));

        let back = Button::with_label("←");
        back.set_tooltip_text(Some("Retour (Alt+←)"));

        let forward = Button::with_label("→");
        forward.set_tooltip_text(Some("Avant (Alt+→)"));

        let reload = Button::with_label("↻");
        reload.set_tooltip_text(Some("Recharger (F5)"));

        let url_entry = Entry::new();
        url_entry.set_placeholder_text(Some("https://…"));
        url_entry.set_hexpand(true);
        url_entry.set_input_purpose(gtk::InputPurpose::Url);

        let bookmark = Button::with_label("★");
        bookmark.set_tooltip_text(Some("Ajouter aux favoris (Ctrl+D)"));

        let eco = Button::with_label("⚡");
        eco.set_tooltip_text(Some("Mode économie : Normal → Éco → Agressif → Ultra (Ctrl+Shift+E)"));

        let eco_label = Label::new(Some("Mode: Normal"));
        let block_label = Label::new(Some("Bloqués: 0"));

        bar.pack_start(&logo, false, false, 0);
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

    fn build_command_bar() -> CommandBarParts {
        let bar = GtkBox::new(Orientation::Vertical, 0);
        bar.style_context().add_class("liteweb-commandbar");

        let hints_label = Label::new(Some(KEYBAR_NORMAL));
        hints_label.set_halign(Align::Start);
        hints_label.set_ellipsize(EllipsizeMode::End);
        hints_label.set_margin_start(8);
        hints_label.set_margin_end(8);
        hints_label.set_margin_top(3);
        hints_label.set_xalign(0.0);
        hints_label
            .style_context()
            .add_class("liteweb-commandbar-hints");

        let input_row = GtkBox::new(Orientation::Horizontal, 4);
        input_row.set_margin_start(6);
        input_row.set_margin_end(6);
        input_row.set_margin_bottom(4);

        let prompt = Label::new(Some(":"));
        prompt
            .style_context()
            .add_class("liteweb-commandbar-prompt");
        prompt.set_width_chars(1);

        let command_entry = Entry::new();
        command_entry.set_placeholder_text(Some("open example.com  |  tab new [url]  |  tab next"));
        command_entry.set_hexpand(true);
        command_entry
            .style_context()
            .add_class("liteweb-commandbar-entry");

        input_row.pack_start(&prompt, false, false, 0);
        input_row.pack_start(&command_entry, true, true, 0);

        bar.pack_start(&hints_label, false, false, 0);
        bar.pack_start(&input_row, false, false, 0);

        Self::apply_command_bar_style();

        CommandBarParts {
            bar,
            command_entry,
            hints_label,
        }
    }

    fn logo_pixbuf(size: i32) -> Option<Pixbuf> {
        let loader = PixbufLoader::new();
        loader.write(LOGO_BYTES).ok()?;
        loader.close().ok()?;
        loader
            .pixbuf()?
            .scale_simple(size, size, InterpType::Bilinear)
    }

    fn apply_command_bar_style() {
        let provider = gtk::CssProvider::new();
        if provider
            .load_from_data(
                b".liteweb-commandbar { background-color: #1e1e1e; border-top: 1px solid #333; } \
                  .liteweb-commandbar-hints { color: #666; font-family: monospace; font-size: 10px; } \
                  .liteweb-commandbar-prompt { color: #7ec8e3; font-family: monospace; font-size: 13px; font-weight: bold; } \
                  .liteweb-commandbar-entry { color: #c8c8c8; font-family: monospace; font-size: 13px; background-color: #2a2a2a; }",
            )
            .is_err()
        {
            return;
        }
        if let Some(screen) = gdk::Screen::default() {
            gtk::StyleContext::add_provider_for_screen(
                &screen,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    fn set_hints(state: Rc<RefCell<AppState>>, hint: &str) {
        state.borrow().hints_label.set_text(hint);
    }

    fn update_hints(state: Rc<RefCell<AppState>>) {
        let (url_focused, cmd_focused) = {
            let st = state.borrow();
            (st.url_entry.has_focus(), st.command_entry.has_focus())
        };
        let hint = if cmd_focused {
            KEYBAR_COMMAND
        } else if url_focused {
            KEYBAR_URL
        } else {
            KEYBAR_NORMAL
        };
        Self::set_hints(state, hint);
    }

    fn focus_command_bar(state: Rc<RefCell<AppState>>) {
        let entry = state.borrow().command_entry.clone();
        entry.grab_focus();
        if entry.text().is_empty() {
            entry.set_position(0);
        }
        Self::set_hints(state, KEYBAR_COMMAND);
    }

    fn wire_command_bar(command_bar: &CommandBarParts, state: Rc<RefCell<AppState>>) {
        let state_cmd = state.clone();
        command_bar.command_entry.connect_activate(move |entry| {
            let text = entry.text().to_string();
            if !text.trim().is_empty() {
                let cmd = if text.starts_with(':') {
                    text
                } else {
                    format!(":{text}")
                };
                Self::run_command(state_cmd.clone(), &cmd);
            }
            entry.set_text("");
            Self::set_hints(state_cmd.clone(), KEYBAR_NORMAL);
        });

        let state_focus = state.clone();
        command_bar
            .command_entry
            .connect_focus_in_event(move |_, _| {
                Self::set_hints(state_focus.clone(), KEYBAR_COMMAND);
                Inhibit(false)
            });

        let state_blur = state.clone();
        command_bar
            .command_entry
            .connect_focus_out_event(move |_, _| {
                let state = state_blur.clone();
                glib::idle_add_local_once(move || {
                    Self::update_hints(state);
                });
                Inhibit(false)
            });

        let state_url_focus = state.clone();
        state
            .borrow()
            .url_entry
            .connect_focus_in_event(move |_, _| {
                Self::set_hints(state_url_focus.clone(), KEYBAR_URL);
                Inhibit(false)
            });

        let state_url_blur = state.clone();
        state
            .borrow()
            .url_entry
            .connect_focus_out_event(move |_, _| {
                let state = state_url_blur.clone();
                glib::idle_add_local_once(move || {
                    Self::update_hints(state);
                });
                Inhibit(false)
            });
    }

    fn wire_toolbar(toolbar: &ToolbarParts, state: Rc<RefCell<AppState>>) {
        let state_back = state.clone();
        toolbar
            .back
            .connect_clicked(move |_| Self::navigate_back(state_back.clone()));

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
            Self::navigate_to(state_submit.clone(), &entry.text().to_string());
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
                Self::focus_command_bar(state.clone());
                return Inhibit(true);
            }
            if key == gtk::gdk::keys::constants::Escape {
                let cmd = state.borrow().command_entry.clone();
                let url = state.borrow().url_entry.clone();
                if cmd.has_focus() {
                    cmd.set_text("");
                    Self::set_hints(state.clone(), KEYBAR_NORMAL);
                    return Inhibit(true);
                }
                if url.has_focus() {
                    url.set_text("");
                    Self::set_hints(state.clone(), KEYBAR_NORMAL);
                    return Inhibit(true);
                }
            }

            Inhibit(false)
        });
    }

    fn wire_notebook(notebook: &Notebook, state: Rc<RefCell<AppState>>) {
        notebook.connect_switch_page(move |_nb, _page, page_num| {
            let index = page_num as usize;
            let state = state.clone();
            // Defer so we run after notebook_sync critical sections that scheduled
            // this emission (rebuild / set_current_page).
            glib::idle_add_local_once(move || {
                if state.borrow().notebook_sync_depth > 0 {
                    return;
                }

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
                    // Real user selection of a suspended tab: restore it.
                    {
                        let mut st = state.borrow_mut();
                        if let Some(tab) = st.tabs.tabs_mut().get_mut(index) {
                            tab.wake();
                        }
                        // wake left the tab Background; mark it active + touch now.
                        st.tabs.set_active(index);
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
            Self::with_notebook_sync(state.clone(), || {
                state.borrow().notebook.set_current_page(Some(index as u32));
            });
        }
        Self::update_chrome(state);
    }

    /// Suppress switch-page side effects around programmatic Notebook mutations.
    /// Depth is released on a later idle so deferred switch-page handlers
    /// scheduled during this section still see a non-zero depth.
    fn with_notebook_sync(state: Rc<RefCell<AppState>>, f: impl FnOnce()) {
        Self::begin_notebook_sync(&state);
        f();
        Self::end_notebook_sync_later(state);
    }

    fn begin_notebook_sync(state: &Rc<RefCell<AppState>>) {
        let mut st = state.borrow_mut();
        st.notebook_sync_depth = st.notebook_sync_depth.saturating_add(1);
    }

    fn end_notebook_sync_later(state: Rc<RefCell<AppState>>) {
        // Two idle turns so same-batch switch-page handlers (one idle each)
        // still observe a non-zero depth before we release.
        glib::idle_add_local_once(move || {
            glib::idle_add_local_once(move || {
                let mut st = state.borrow_mut();
                st.notebook_sync_depth = st.notebook_sync_depth.saturating_sub(1);
            });
        });
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

        Self::begin_notebook_sync(&state);

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
            let user_content = st.user_content.clone();
            let policy = st.energy.level().webview_policy();
            let storage = st.storage.clone();
            let state_for_tabs = state.clone();

            for tab in st.tabs.tabs_mut().iter_mut() {
                if tab.is_suspended() || tab.webview.is_some() {
                    continue;
                }
                let wv = create_webview(&web_context, blocker.clone(), &user_content, policy);
                Self::connect_webview(wv.clone(), tab.id, state_for_tabs.clone(), storage.clone());
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
            let user_content = st.user_content.clone();
            let policy = st.energy.level().webview_policy();
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
                        let wv = create_webview(
                            &web_context,
                            blocker.clone(),
                            &user_content,
                            policy,
                        );
                        Self::connect_webview(
                            wv.clone(),
                            tab.id,
                            state_for_tabs.clone(),
                            storage.clone(),
                        );
                        tab.webview = Some(wv);
                    }

                    if let Some(wv) = &tab.webview {
                        // WebKit2 WebView scrolls itself. Wrapping it in
                        // GtkScrolledWindow swallows wheel events once Ultra
                        // disables GPU compositing (software path uses GTK
                        // routing, and the child already fills the viewport).
                        wv.set_hexpand(true);
                        wv.set_vexpand(true);
                        page_box.pack_start(wv, true, true, 0);

                        let url = tab.url.clone();
                        if is_safe_navigation_url(&url) && url != "about:blank" {
                            let wv = wv.clone();
                            glib::idle_add_local_once(move || {
                                wv.load_uri(&url);
                            });
                        }
                    }
                }

                st.notebook.append_page(&page_box, Some(&label));
                page_box.show_all();
                label.show();
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

        // Release this render's sync depth after deferred switch-page handlers.
        Self::end_notebook_sync_later(state);
    }

    fn connect_webview(wv: WebView, tab_id: usize, state: Rc<RefCell<AppState>>, storage: Storage) {
        let state_title = state.clone();
        wv.connect_title_notify(move |view| {
            if let Some(title) = view.title() {
                {
                    let mut st = state_title.borrow_mut();
                    if let Some(tab_index) = st.tabs.index_of_id(tab_id) {
                        let tab = &mut st.tabs.tabs_mut()[tab_index];
                        tab.title = title.to_string();
                    }
                }
                let tab_index = { state_title.borrow().tabs.index_of_id(tab_id) };
                if let Some(tab_index) = tab_index {
                    Self::update_tab_label(state_title.clone(), tab_index);
                }
            }
        });

        let state_load = state.clone();
        wv.connect_load_changed(move |view, event| {
            if event != LoadEvent::Finished {
                return;
            }
            let uri = view.uri();
            let title = view.title();
            let view = view.clone();
            let storage = storage.clone();
            let state_load = state_load.clone();
            glib::idle_add_local_once(move || {
                let Some(uri) = uri else {
                    return;
                };
                if !is_safe_navigation_url(&uri) {
                    return;
                }
                let title = title.unwrap_or_else(|| uri.clone());
                storage.add_history(&uri, &title);
                let flatten = {
                    let mut st = state_load.borrow_mut();
                    if let Some(tab_index) = st.tabs.index_of_id(tab_id) {
                        let tab = &mut st.tabs.tabs_mut()[tab_index];
                        tab.url = uri.to_string();
                        tab.title = title.to_string();
                        tab.modified = false;
                    }
                    if st.tabs.active_tab().map(|tab| tab.id) == Some(tab_id) {
                        st.url_entry.set_text(&uri);
                    }
                    st.block_label
                        .set_text(&format!("Bloqués: {}", st.blocker.blocked_count()));
                    st.energy.level().webview_policy().flatten_document
                };
                if flatten {
                    Self::flatten_finished_load(view, uri.to_string(), tab_id, state_load);
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

        if !is_safe_navigation_url(&url) {
            Self::show_message(
                "Navigation refusée",
                "LiteWeb autorise uniquement HTTP, HTTPS et about:blank.",
            );
            return;
        }

        let mut st = state.borrow_mut();
        let idx = st.tabs.active_index();
        if let Some(tab) = st.tabs.tabs_mut().get_mut(idx) {
            tab.url = url.clone();
            tab.wake();
            tab.reader_pending = false;
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
        if let Some(tab) = state.borrow_mut().tabs.active_tab_mut() {
            tab.reader_pending = false;
        }
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
        if !is_safe_navigation_url(url) {
            Self::show_message(
                "Navigation refusée",
                "LiteWeb autorise uniquement HTTP, HTTPS et about:blank.",
            );
            return;
        }
        state.borrow_mut().tabs.create_tab(url);
        // Incremental append — do not structural-rebuild (that destroys current tabs).
        Self::render_tabs(state.clone(), false);
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
        state.borrow_mut().energy.toggle();
        Self::apply_energy_to_live_tabs(state.clone());
        Self::update_chrome(state);
    }

    fn apply_energy_to_live_tabs(state: Rc<RefCell<AppState>>) {
        let (policy, webviews, content) = {
            let st = state.borrow();
            let policy = st.energy.level().webview_policy();
            let webviews: Vec<WebView> = st
                .tabs
                .tabs()
                .iter()
                .filter_map(|tab| tab.webview.clone())
                .collect();
            (policy, webviews, st.user_content.clone())
        };
        set_archaic_stylesheet(&content, policy.archaic_stylesheet);
        for webview in &webviews {
            apply_webview_policy(webview, policy);
        }
        {
            let mut st = state.borrow_mut();
            for tab in st.tabs.tabs_mut() {
                tab.reader_pending = false;
            }
        }
        for webview in webviews {
            if let Some(uri) = webview.uri() {
                if is_safe_navigation_url(&uri) && uri != "about:blank" {
                    webview.reload();
                }
            }
        }
    }

    fn flatten_finished_load(
        view: WebView,
        uri: String,
        tab_id: usize,
        state: Rc<RefCell<AppState>>,
    ) {
        if uri == "about:blank" {
            return;
        }

        let already_flat = {
            let st = state.borrow();
            st.tabs
                .index_of_id(tab_id)
                .and_then(|index| st.tabs.tabs().get(index))
                .map(|tab| tab.reader_pending)
                .unwrap_or(false)
        };
        if already_flat {
            Self::set_reader_pending(&state, tab_id, false);
            return;
        }

        let Some(resource) = view.main_resource() else {
            Self::replace_with_reader(view, String::new(), uri, tab_id, state);
            return;
        };

        WebResourceExt::data(&resource, None::<&Cancellable>, move |result| {
            let html = result
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            // WebKit may invoke this trampoline while a RefCell borrow is
            // still live; finish the replace on the next idle turn.
            glib::idle_add_local_once(move || {
                Self::replace_with_reader(view, html, uri, tab_id, state);
            });
        });
    }

    fn set_reader_pending(state: &Rc<RefCell<AppState>>, tab_id: usize, pending: bool) {
        let index = state.borrow().tabs.index_of_id(tab_id);
        if let Some(index) = index {
            if let Some(tab) = state.borrow_mut().tabs.tabs_mut().get_mut(index) {
                tab.reader_pending = pending;
            }
        }
    }

    fn replace_with_reader(
        view: WebView,
        html: String,
        uri: String,
        tab_id: usize,
        state: Rc<RefCell<AppState>>,
    ) {
        let flat = flatten_html(&html, &uri);
        Self::set_reader_pending(&state, tab_id, true);
        view.load_html(&flat, Some(&uri));
        view.grab_focus();
    }

    fn run_command(state: Rc<RefCell<AppState>>, input: &str) {
        let state_cmd = state.clone();
        match CommandPalette::parse(input) {
            CommandAction::Open(url) => Self::navigate_to(state_cmd.clone(), &url),
            CommandAction::Tab(n) => {
                Self::switch_to_tab(state_cmd.clone(), n);
            }
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
                Self::apply_energy_to_live_tabs(state_cmd.clone());
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::EcoOff => {
                state_cmd.borrow_mut().energy.set_level(EnergyLevel::Normal);
                Self::apply_energy_to_live_tabs(state_cmd.clone());
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::EcoAggressive => {
                state_cmd
                    .borrow_mut()
                    .energy
                    .set_level(EnergyLevel::Aggressive);
                Self::apply_energy_to_live_tabs(state_cmd.clone());
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::EcoUltra => {
                state_cmd.borrow_mut().energy.set_level(EnergyLevel::Ultra);
                Self::apply_energy_to_live_tabs(state_cmd.clone());
                Self::update_chrome(state_cmd.clone());
            }
            CommandAction::BookmarkAdd => Self::bookmark_current(state_cmd.clone()),
            CommandAction::BookmarkList => Self::show_bookmarks(state_cmd.clone()),
            CommandAction::History => Self::show_history(state_cmd.clone()),
            CommandAction::DownloadList => Self::show_message(
                "Téléchargements",
                "Gestion des téléchargements — bientôt disponible.",
            ),
            CommandAction::Unknown(cmd) => {
                Self::show_message("Commande inconnue", &format!("'{cmd}' n'est pas reconnue."));
            }
        }
        state.borrow().command_entry.set_text("");
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
            let hold_pages = {
                let st = state.borrow();
                st.benchmark
                    .as_ref()
                    .map(|run| !run.reporter.scenario().uses_suspension())
                    .unwrap_or(false)
            };
            // Loaded/Ultra compare live engine cost. Ultra's 15s/1-tab policy
            // would otherwise drop two of the three pages at the first 30s tick.
            if hold_pages {
                return glib::Continue(true);
            }

            // GTK notebook selection emits deferred callbacks during startup.
            // In benchmark runs, force the non-measured blank sentinel as the
            // logical active tab before evaluating inactivity. This keeps all
            // ten workload tabs eligible for the normal suspension policy.
            let sentinel = {
                let st = state.borrow();
                st.benchmark
                    .as_ref()
                    .filter(|run| run.reporter.scenario().uses_suspension())
                    .map(|_| st.tabs.tabs().len().saturating_sub(1))
            };
            if let Some(sentinel) = sentinel {
                state.borrow_mut().tabs.set_active(sentinel);
                let needs_page = state.borrow().notebook.current_page() != Some(sentinel as u32);
                if needs_page {
                    Self::with_notebook_sync(state.clone(), || {
                        state
                            .borrow()
                            .notebook
                            .set_current_page(Some(sentinel as u32));
                    });
                }
            }

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
                if suspended > 0 {
                    Self::render_tabs(state.clone(), true);
                }
            }

            glib::Continue(true)
        });
    }

    fn activate_benchmark_sentinel(state: Rc<RefCell<AppState>>) {
        let has_sentinel = state
            .borrow()
            .benchmark
            .as_ref()
            .map(|run| run.reporter.scenario().uses_suspension())
            .unwrap_or(false);
        if !has_sentinel {
            return;
        }

        // Notebook emits delayed switch-page callbacks while its initial pages
        // are appended. Schedule this after them so the blank sentinel remains
        // selected and every measured URL is genuinely inactive.
        glib::idle_add_local_once(move || {
            let sentinel = state.borrow().tabs.tabs().len().saturating_sub(1);
            Self::switch_to_tab(state, sentinel);
        });
    }

    fn start_benchmark_timer(state: Rc<RefCell<AppState>>, app: Application) {
        if state.borrow().benchmark.is_none() {
            return;
        }

        glib::timeout_add_local(Duration::from_secs(1), move || {
            let should_quit = {
                let mut st = state.borrow_mut();
                let suspended_tabs = st
                    .tabs
                    .tabs()
                    .iter()
                    .filter(|tab| tab.is_suspended())
                    .count();
                let Some(run) = st.benchmark.as_mut() else {
                    return glib::Continue(false);
                };

                let elapsed = run.reporter.elapsed();
                if !run.warmup_reported && elapsed >= Duration::from_secs(WARMUP_SECS) {
                    run.warmup_reported = true;
                    run.reporter.event("warmup_complete", suspended_tabs);
                }

                let expected = run.reporter.scenario().expected_suspended_tabs();
                if expected > 0 && suspended_tabs > 0 && run.first_suspension_at.is_none() {
                    run.first_suspension_at = Some(Instant::now());
                    run.reporter.event("first_suspension", suspended_tabs);
                }
                if expected > 0 && suspended_tabs >= expected && run.all_suspended_at.is_none() {
                    run.all_suspended_at = Some(Instant::now());
                    run.reporter.event("all_suspended", suspended_tabs);
                }

                let complete = match run.reporter.scenario() {
                    BenchmarkScenario::Idle
                    | BenchmarkScenario::Loaded
                    | BenchmarkScenario::Ultra => {
                        elapsed >= Duration::from_secs(WARMUP_SECS + IDLE_MEASUREMENT_SECS)
                    }
                    BenchmarkScenario::Normal | BenchmarkScenario::Aggressive => run
                        .all_suspended_at
                        .map(|at| at.elapsed() >= Duration::from_secs(POST_SUSPENSION_SECS))
                        .unwrap_or(false),
                };
                if complete {
                    run.reporter.event("completed", suspended_tabs);
                }
                complete
            };

            if should_quit {
                app.quit();
                glib::Continue(false)
            } else {
                glib::Continue(true)
            }
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
