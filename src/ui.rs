use crate::config::{self, Config};
use crate::emoji::{self, Item, CATEGORIES};
use crate::emoji_object::EmojiObject;
use crate::frecency::Frecency;
use crate::inject;
use emojis::Group;
use gtk::gdk::ModifierType;
use gtk::glib;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

pub struct AppState {
    pub config: Config,
    pub frecency: Frecency,
    pub oneshot: bool,
}

const RECENTS: usize = 40;
const SEARCH_CAP: usize = 400;

const BASE_WIDTH: f32 = 640.0;
const BASE_HEIGHT: f32 = 480.0;

struct Palette {
    bg: &'static str,
    fg: &'static str,
    entry_bg: &'static str,
    border: &'static str,
    hover: &'static str,
    sel_bg: &'static str,
    sel_fg: &'static str,
}

/// Breeze-matched colors driven by the detected scheme rather than the GTK
/// theme's `@theme_*` named colors — a recycled window that was hidden across a
/// color-scheme switch does not reliably re-resolve those, so we own the palette
/// and just reload this provider on show.
fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: "#232629",
            fg: "#eff0f1",
            entry_bg: "#1b1e20",
            border: "rgba(255,255,255,0.13)",
            hover: "rgba(238,240,241,0.09)",
            sel_bg: "#3daee9",
            sel_fg: "#ffffff",
        }
    } else {
        Palette {
            bg: "#eff0f1",
            fg: "#232629",
            entry_bg: "#ffffff",
            border: "rgba(0,0,0,0.15)",
            hover: "rgba(35,38,41,0.08)",
            sel_bg: "#3daee9",
            sel_fg: "#ffffff",
        }
    }
}

fn css_for(scale: f32, dark: bool) -> String {
    let p = palette(dark);
    format!(
        "window.emojipick, .emojipick > box {{
  background: {bg};
  color: {fg};
  border-radius: 14px;
  border: 1px solid {border};
  box-shadow: 0 12px 40px alpha(black, 0.45);
}}
.emojipick label {{ color: {fg}; }}
.search-entry {{ font-size: {search}rem; margin: 4px; color: {fg}; background: {entry_bg}; }}
.search-entry text {{ color: {fg}; }}
.search-entry image {{ color: {fg}; }}
.emoji-preview {{ font-size: {preview}rem; }}
.preview-name {{ font-weight: bold; font-size: {name}rem; }}
.shortcode {{ opacity: 0.55; font-size: {shortcode}rem; }}
.tone-btn {{ background: transparent; }}
.tone-btn label {{ font-size: {tone}rem; }}
.category-bar.linked button {{ padding: 4px 10px; font-size: {cat}rem; background: transparent; color: {fg}; }}
.category-bar button:checked {{
  background: {sel_bg};
  color: {sel_fg};
}}
.emoji-glyph {{
  font-family: \"Noto Color Emoji\", \"Twemoji\", emoji;
  font-size: {glyph}rem;
}}
.emoji-grid > * {{
  border-radius: 8px; padding: 2px;
  min-width: {cell}px; min-height: {cell}px;
  transition: background 120ms ease;
}}
.emoji-grid > *:hover  {{ background: {hover}; }}
.emoji-grid > *:selected {{
  background: {sel_bg};
  outline: 2px solid {sel_bg}; outline-offset: -2px;
}}
.empty-state {{ opacity: 0.5; font-size: {empty}rem; }}
",
        bg = p.bg,
        fg = p.fg,
        entry_bg = p.entry_bg,
        border = p.border,
        hover = p.hover,
        sel_bg = p.sel_bg,
        sel_fg = p.sel_fg,
        search = 1.05 * scale,
        preview = 2.2 * scale,
        name = 1.0 * scale,
        shortcode = 0.85 * scale,
        tone = 1.3 * scale,
        cat = 1.0 * scale,
        glyph = 1.6 * scale,
        cell = (44.0 * scale) as i32,
        empty = 1.1 * scale,
    )
}

fn recents_items(state: &Rc<RefCell<AppState>>) -> Vec<Item> {
    let (tops, tone) = {
        let s = state.borrow();
        (s.frecency.recent_n(RECENTS), s.config.skin_tone)
    };
    tops.into_iter()
        .filter_map(|base| emoji::item_for_glyph(&base, tone))
        .collect()
}

fn items_for(query: &str, category: Option<Group>, state: &Rc<RefCell<AppState>>) -> Vec<Item> {
    let query = query.trim();
    let tone = state.borrow().config.skin_tone;

    if !query.is_empty() {
        let mut items = emoji::search(query, tone, &state.borrow().frecency);
        items.truncate(SEARCH_CAP);
        return items;
    }

    if let Some(group) = category {
        return emoji::by_group(group, tone);
    }

    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for item in recents_items(state) {
        if seen.insert(item.glyph.clone()) {
            items.push(item);
        }
    }
    for item in emoji::all(tone) {
        if seen.insert(item.glyph.clone()) {
            items.push(item);
        }
    }
    items
}

fn on_pick(base: &str, glyph: &str, window: &ApplicationWindow, state: &Rc<RefCell<AppState>>) {
    let (oneshot, config) = {
        let mut s = state.borrow_mut();
        s.frecency.record(base);
        let _ = s.frecency.save();
        (s.oneshot, s.config.clone())
    };
    hide(window);
    let glyph = glyph.to_string();
    let window = window.clone();
    glib::timeout_add_local_once(Duration::from_millis(60), move || {
        if let Err(err) = inject::insert(&glyph, &config, oneshot) {
            eprintln!("emojipick: failed to insert emoji: {err:#}");
        }
        if oneshot {
            if let Some(app) = window.application() {
                app.quit();
            }
        }
    });
}

fn hand_tone(tone: u8) -> String {
    emojis::get("\u{270B}")
        .map(|e| emoji::apply_skin_tone(e, tone))
        .unwrap_or_else(|| "\u{270B}".to_string())
}

fn focus_in_entry(window: &ApplicationWindow, entry: &gtk::SearchEntry) -> bool {
    match GtkWindowExt::focus(window) {
        Some(w) => w.eq(entry.upcast_ref::<gtk::Widget>()) || w.is_ancestor(entry),
        None => false,
    }
}

pub fn build_window(app: &Application, state: Rc<RefCell<AppState>>) -> ApplicationWindow {
    let scale0 = state.borrow().config.scale;
    let columns = state.borrow().config.grid_columns.max(1) as i32;

    let window = ApplicationWindow::builder()
        .application(app)
        .title("emojipick")
        .default_width((BASE_WIDTH * scale0) as i32)
        .default_height((BASE_HEIGHT * scale0) as i32)
        .decorated(false)
        .resizable(false)
        .build();
    window.add_css_class("emojipick");

    let dark = Rc::new(Cell::new(false));

    let provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let restyle: Rc<dyn Fn()> = {
        let provider = provider.clone();
        let window = window.clone();
        let state = state.clone();
        let dark = dark.clone();
        Rc::new(move || {
            let scale = state.borrow().config.scale;
            provider.load_from_string(&css_for(scale, dark.get()));
            window.set_default_size((BASE_WIDTH * scale) as i32, (BASE_HEIGHT * scale) as i32);
        })
    };

    crate::theme::watch({
        let dark = dark.clone();
        let restyle = restyle.clone();
        move |is_dark| {
            dark.set(is_dark);
            restyle();
        }
    });

    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let preview = gtk::Label::new(None);
    preview.add_css_class("emoji-preview");
    let name_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    name_box.set_hexpand(true);
    name_box.set_valign(gtk::Align::Center);
    let name_label = gtk::Label::new(None);
    name_label.add_css_class("preview-name");
    name_label.set_halign(gtk::Align::Start);
    name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let shortcode_label = gtk::Label::new(None);
    shortcode_label.add_css_class("shortcode");
    shortcode_label.set_halign(gtk::Align::Start);
    name_box.append(&name_label);
    name_box.append(&shortcode_label);

    let tone_btn = gtk::MenuButton::new();
    tone_btn.add_css_class("tone-btn");
    tone_btn.set_label(&hand_tone(state.borrow().config.skin_tone));
    tone_btn.set_tooltip_text(Some("Skin tone (Ctrl+0\u{2013}5)"));
    let tone_popover = gtk::Popover::new();
    let tone_row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    tone_popover.set_child(Some(&tone_row));
    tone_btn.set_popover(Some(&tone_popover));

    header.append(&preview);
    header.append(&name_box);
    header.append(&tone_btn);
    root.append(&header);

    let entry = gtk::SearchEntry::new();
    entry.add_css_class("search-entry");
    entry.set_placeholder_text(Some("Search emoji\u{2026}"));
    entry.set_hexpand(true);
    entry.set_search_delay(0);
    root.append(&entry);

    let category_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    category_bar.add_css_class("category-bar");
    category_bar.add_css_class("linked");
    let category_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&category_bar)
        .build();
    root.append(&category_scroll);

    let store = gtk::gio::ListStore::new::<EmojiObject>();
    let sel = gtk::SingleSelection::new(Some(store.clone()));
    sel.set_can_unselect(false);
    sel.set_autoselect(true);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, list_item| {
        let label = gtk::Label::new(None);
        label.add_css_class("emoji-glyph");
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        list_item.set_child(Some(&label));
    });
    factory.connect_bind(|_, list_item| {
        let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = list_item.item().and_downcast::<EmojiObject>().unwrap();
        let label = list_item.child().and_downcast::<gtk::Label>().unwrap();
        label.set_label(&obj.glyph());
        label.set_tooltip_text(Some(&obj.name()));
        label.update_property(&[gtk::accessible::Property::Label(&obj.name())]);
    });

    let grid = gtk::GridView::new(Some(sel.clone()), Some(factory));
    grid.set_max_columns(columns as u32);
    grid.set_min_columns(columns as u32);
    grid.set_enable_rubberband(false);
    grid.set_single_click_activate(true);
    grid.add_css_class("emoji-grid");

    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .vexpand(true)
        .child(&grid)
        .build();

    let empty_label = gtk::Label::new(Some("No emoji found"));
    empty_label.add_css_class("empty-state");
    empty_label.set_vexpand(true);
    empty_label.set_valign(gtk::Align::Center);
    empty_label.set_halign(gtk::Align::Center);

    let stack = gtk::Stack::new();
    stack.set_vexpand(true);
    stack.add_named(&scroller, Some("grid"));
    stack.add_named(&empty_label, Some("empty"));
    root.append(&stack);

    window.set_child(Some(&root));

    let current_category: Rc<RefCell<Option<Group>>> = Rc::new(RefCell::new(None));
    let category_toggles: Rc<RefCell<Vec<gtk::ToggleButton>>> = Rc::new(RefCell::new(Vec::new()));

    let update_header: Rc<dyn Fn()> = {
        let preview = preview.clone();
        let name_label = name_label.clone();
        let shortcode_label = shortcode_label.clone();
        let sel = sel.clone();
        Rc::new(move || {
            match sel.selected_item().and_downcast::<EmojiObject>() {
                Some(obj) => {
                    preview.set_label(&obj.glyph());
                    name_label.set_label(&obj.name());
                    shortcode_label.set_label(&obj.shortcode());
                }
                None => {
                    preview.set_label("");
                    name_label.set_label("");
                    shortcode_label.set_label("");
                }
            }
        })
    };

    {
        let update_header = update_header.clone();
        sel.connect_selected_item_notify(move |_| update_header());
    }

    let rebuild: Rc<dyn Fn()> = {
        let entry = entry.clone();
        let state = state.clone();
        let current_category = current_category.clone();
        let store = store.clone();
        let sel = sel.clone();
        let grid = grid.clone();
        let stack = stack.clone();
        let scroller = scroller.clone();
        let update_header = update_header.clone();
        Rc::new(move || {
            let query = entry.text().to_string();
            let category = *current_category.borrow();
            let items = items_for(&query, category, &state);

            let objects: Vec<EmojiObject> = items
                .iter()
                .map(|i| EmojiObject::new(&i.glyph, &i.base, &i.name, &i.shortcode))
                .collect();

            store.splice(0, store.n_items(), &objects);

            if objects.is_empty() {
                stack.set_visible_child_name("empty");
            } else {
                stack.set_visible_child_name("grid");
                sel.set_selected(0);
                grid.scroll_to(0, gtk::ListScrollFlags::SELECT, None);
                scroller.vadjustment().set_value(0.0);
            }
            update_header();
        })
    };

    {
        let recents_btn = gtk::ToggleButton::with_label("Recents");
        recents_btn.set_active(true);
        {
            let rebuild = rebuild.clone();
            let current_category = current_category.clone();
            let entry = entry.clone();
            recents_btn.connect_clicked(move |_| {
                *current_category.borrow_mut() = None;
                entry.set_text("");
                rebuild();
                entry.grab_focus();
            });
        }
        category_bar.append(&recents_btn);
        category_toggles.borrow_mut().push(recents_btn.clone());

        for (label, group) in CATEGORIES {
            let button = gtk::ToggleButton::with_label(label);
            button.set_group(Some(&recents_btn));
            let rebuild = rebuild.clone();
            let current_category = current_category.clone();
            let entry = entry.clone();
            let group = *group;
            button.connect_clicked(move |_| {
                *current_category.borrow_mut() = Some(group);
                entry.set_text("");
                rebuild();
                entry.grab_focus();
            });
            category_bar.append(&button);
            category_toggles.borrow_mut().push(button);
        }
    }

    for tone in 0u8..=5 {
        let swatch = gtk::Button::with_label(&hand_tone(tone));
        swatch.add_css_class("flat");
        let state = state.clone();
        let rebuild = rebuild.clone();
        let tone_btn = tone_btn.clone();
        let tone_popover = tone_popover.clone();
        swatch.connect_clicked(move |_| {
            {
                let mut s = state.borrow_mut();
                s.config.skin_tone = tone;
                let _ = s.config.save();
            }
            tone_btn.set_label(&hand_tone(tone));
            tone_popover.popdown();
            rebuild();
        });
        tone_row.append(&swatch);
    }

    {
        let rebuild = rebuild.clone();
        entry.connect_search_changed(move |_| {
            rebuild();
        });
    }

    {
        let window = window.clone();
        let state = state.clone();
        let sel = sel.clone();
        entry.connect_activate(move |_| {
            if let Some(obj) = sel.selected_item().and_downcast::<EmojiObject>() {
                on_pick(&obj.base(), &obj.glyph(), &window, &state);
            }
        });
    }

    {
        let window = window.clone();
        let state = state.clone();
        grid.connect_activate(move |gv, pos| {
            if let Some(obj) = gv
                .model()
                .and_then(|m| m.item(pos))
                .and_downcast::<EmojiObject>()
            {
                on_pick(&obj.base(), &obj.glyph(), &window, &state);
            }
        });
    }

    {
        let entry_k = entry.clone();
        let window_k = window.clone();
        let grid_k = grid.clone();
        let scroller_k = scroller.clone();
        let sel_k = sel.clone();
        let state_k = state.clone();
        let current_category = current_category.clone();
        let category_toggles = category_toggles.clone();
        let rebuild = rebuild.clone();
        let tone_btn_k = tone_btn.clone();
        let restyle_k = restyle.clone();

        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        key.connect_key_pressed(move |_, keyval, _, modifiers| {
            use gtk::gdk::Key;
            let in_entry = focus_in_entry(&window_k, &entry_k);
            let ctrl = modifiers.contains(ModifierType::CONTROL_MASK);
            let alt = modifiers.contains(ModifierType::ALT_MASK);

            if keyval == Key::Escape {
                let has_query = !entry_k.text().is_empty();
                let has_category = current_category.borrow().is_some();
                if has_query || has_category || !in_entry {
                    entry_k.set_text("");
                    *current_category.borrow_mut() = None;
                    if let Some(first) = category_toggles.borrow().first() {
                        first.set_active(true);
                    }
                    rebuild();
                    entry_k.grab_focus();
                } else if state_k.borrow().oneshot {
                    window_k.close();
                } else {
                    hide(&window_k);
                }
                return glib::Propagation::Stop;
            }

            if ctrl {
                let zoom = match keyval {
                    Key::plus | Key::equal | Key::KP_Add => Some(config::SCALE_STEP),
                    Key::minus | Key::underscore | Key::KP_Subtract => Some(-config::SCALE_STEP),
                    _ => None,
                };
                if let Some(delta) = zoom {
                    {
                        let mut s = state_k.borrow_mut();
                        s.config.scale = config::round_scale(
                            (s.config.scale + delta).clamp(config::SCALE_MIN, config::SCALE_MAX),
                        );
                        let _ = s.config.save();
                    }
                    restyle_k();
                    return glib::Propagation::Stop;
                }

                let nav = match keyval {
                    Key::h => Some(-1i32),
                    Key::l => Some(1),
                    Key::j => Some(columns),
                    Key::k => Some(-columns),
                    _ => None,
                };
                if let Some(delta) = nav {
                    let count = sel_k.n_items() as i32;
                    if count > 0 {
                        let next = (sel_k.selected() as i32 + delta).clamp(0, count - 1) as u32;
                        sel_k.set_selected(next);
                        grid_k.grab_focus();
                        grid_k.scroll_to(
                            next,
                            gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                            None,
                        );
                    }
                    return glib::Propagation::Stop;
                }

                let page_nav = match keyval {
                    Key::d => Some(columns * 3),
                    Key::u => Some(-columns * 3),
                    Key::f => Some(columns * 6),
                    Key::b => Some(-columns * 6),
                    _ => None,
                };
                if let Some(delta) = page_nav {
                    let count = sel_k.n_items() as i32;
                    if count > 0 {
                        let target = (sel_k.selected() as i32 + delta).clamp(0, count - 1) as u32;
                        sel_k.set_selected(target);
                        grid_k.grab_focus();
                        grid_k.scroll_to(
                            target,
                            gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                            None,
                        );
                    }
                    return glib::Propagation::Stop;
                }

                let line_scroll = match keyval {
                    Key::e => Some(1.0f64),
                    Key::y => Some(-1.0),
                    _ => None,
                };
                if let Some(dir) = line_scroll {
                    let adj = scroller_k.vadjustment();
                    let max = (adj.upper() - adj.page_size()).max(adj.lower());
                    let next = (adj.value() + dir * adj.step_increment()).clamp(adj.lower(), max);
                    adj.set_value(next);
                    return glib::Propagation::Stop;
                }

                let tone = match keyval {
                    Key::_0 => Some(0u8),
                    Key::_1 => Some(1),
                    Key::_2 => Some(2),
                    Key::_3 => Some(3),
                    Key::_4 => Some(4),
                    Key::_5 => Some(5),
                    _ => None,
                };
                if let Some(tone) = tone {
                    {
                        let mut s = state_k.borrow_mut();
                        s.config.skin_tone = tone;
                        let _ = s.config.save();
                    }
                    tone_btn_k.set_label(&hand_tone(tone));
                    rebuild();
                    return glib::Propagation::Stop;
                }
                return glib::Propagation::Proceed;
            }

            if keyval == Key::Tab || keyval == Key::ISO_Left_Tab {
                let delta = if keyval == Key::ISO_Left_Tab
                    || modifiers.contains(ModifierType::SHIFT_MASK)
                {
                    -1
                } else {
                    1
                };
                let toggles = category_toggles.borrow();
                let n = toggles.len() as i32;
                if n > 0 {
                    let cur = toggles.iter().position(|t| t.is_active()).unwrap_or(0) as i32;
                    let next = (cur + delta).rem_euclid(n) as usize;
                    toggles[next].set_active(true);
                    *current_category.borrow_mut() = if next == 0 {
                        None
                    } else {
                        Some(CATEGORIES[next - 1].1)
                    };
                    drop(toggles);
                    entry_k.set_text("");
                    rebuild();
                    entry_k.grab_focus();
                }
                return glib::Propagation::Stop;
            }

            if in_entry && keyval == Key::Down {
                grid_k.grab_focus();
                sel_k.set_selected(0);
                grid_k.scroll_to(
                    0,
                    gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                    None,
                );
                return glib::Propagation::Stop;
            }

            if !in_entry && keyval == Key::Up {
                let selected = sel_k.selected();
                if selected < columns as u32 {
                    entry_k.grab_focus();
                    entry_k.set_position(-1);
                    return glib::Propagation::Stop;
                }
            }

            {
                let count = sel_k.n_items() as i32;
                if count > 0 {
                    let page = columns * 5;
                    let entry_empty = entry_k.text().is_empty();
                    let target = match keyval {
                        Key::Home if !in_entry || entry_empty => Some(0),
                        Key::End if !in_entry || entry_empty => Some(count - 1),
                        Key::Page_Up => Some(sel_k.selected() as i32 - page),
                        Key::Page_Down => Some(sel_k.selected() as i32 + page),
                        _ => None,
                    };
                    if let Some(t) = target {
                        let t = t.clamp(0, count - 1) as u32;
                        sel_k.set_selected(t);
                        grid_k.grab_focus();
                        grid_k.scroll_to(
                            t,
                            gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                            None,
                        );
                        return glib::Propagation::Stop;
                    }
                }
            }

            if !in_entry && keyval == Key::BackSpace {
                let mut text = entry_k.text().to_string();
                text.pop();
                entry_k.set_text(&text);
                entry_k.grab_focus();
                entry_k.set_position(-1);
                return glib::Propagation::Stop;
            }

            if !in_entry && keyval == Key::space {
                if let Some(obj) = sel_k.selected_item().and_downcast::<EmojiObject>() {
                    on_pick(&obj.base(), &obj.glyph(), &window_k, &state_k);
                }
                return glib::Propagation::Stop;
            }

            if !in_entry && !ctrl && !alt {
                if let Some(c) = keyval.to_unicode() {
                    if !c.is_control() {
                        let text = format!("{}{}", entry_k.text(), c);
                        entry_k.set_text(&text);
                        entry_k.grab_focus();
                        entry_k.set_position(-1);
                        return glib::Propagation::Stop;
                    }
                }
            }

            glib::Propagation::Proceed
        });
        window.add_controller(key);
    }

    if !state.borrow().oneshot {
        window.connect_notify_local(Some("is-active"), |w, _| {
            if !w.is_active() {
                hide(w);
            }
        });
    }

    {
        let entry = entry.clone();
        let current_category = current_category.clone();
        let category_toggles = category_toggles.clone();
        let rebuild = rebuild.clone();
        let sel = sel.clone();
        let grid = grid.clone();
        let scroller = scroller.clone();
        let restyle = restyle.clone();
        window.connect_map(move |_| {
            *current_category.borrow_mut() = None;
            if let Some(first) = category_toggles.borrow().first() {
                first.set_active(true);
            }
            entry.set_text("");
            rebuild();
            let entry = entry.clone();
            let sel = sel.clone();
            let grid = grid.clone();
            let scroller = scroller.clone();
            let restyle = restyle.clone();
            glib::idle_add_local_once(move || {
                restyle();
                if sel.n_items() > 0 {
                    sel.set_selected(0);
                    grid.scroll_to(
                        0,
                        gtk::ListScrollFlags::FOCUS | gtk::ListScrollFlags::SELECT,
                        None,
                    );
                }
                scroller.vadjustment().set_value(0.0);
                entry.grab_focus();
            });
        });
    }

    rebuild();
    window
}

pub fn show(window: &ApplicationWindow) {
    window.set_visible(true);
    window.present();
}

pub fn hide(window: &ApplicationWindow) {
    window.set_visible(false);
}
