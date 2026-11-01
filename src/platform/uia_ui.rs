//! The UI-thread half of UI Automation: turn the page's rows into elements, and carry out
//! "press this" and "focus this". `uia.rs` (the COM half) only ever talks to this through a
//! window message, so none of this is shared across threads.
//!
//! Every element is built from the same row model the painter uses, so what a screen reader
//! announces is what is on screen. Keys are stable across repaints: a hash of the page and the
//! element's role and label (plus a counter when the same label appears twice).

use std::collections::HashMap;

use super::super::uia::{self, Info, Op, Query, Role};
use super::*;

/// What pressing an element does. Mirrors what a click does, through the same functions.
#[derive(Clone, Copy)]
enum Do {
    Nothing,
    Nav(Page),
    Toggle(Toggle),
    Choice(Choice),
    Button(Button),
    Preset(usize),
    Window(usize),
    Recipe(usize),
    Look(usize),
    Wall(usize),
    Rule(usize),
    Exclude(usize),
    TrialKeep,
    TrialRevert,
    Credit,
    FocusSearch,
    Go(Page),
    Result(usize),
    Section(&'static str),
}

struct Built {
    info: Info,
    action: Do,
    /// Row index in `rows()` this element belongs to, and its position inside that row.
    row: Option<(usize, usize)>,
}

fn fnv64(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    // Zero is the window root's key.
    if hash == uia::ROOT_KEY {
        1
    } else {
        hash
    }
}

struct Builder2<'a> {
    ui: &'a Ui,
    hwnd: HWND,
    out: Vec<Built>,
    seen: HashMap<String, u32>,
    /// Top and bottom of the visible part of the page, in screen pixels. A content element wholly
    /// outside it is reported off-screen.
    view: (f64, f64),
}

impl Builder2<'_> {
    /// Rectangle in content logical px to a screen rectangle.
    fn screen(&self, l: i32, t: i32, r: i32, b: i32) -> [f64; 4] {
        let k = self.ui.k;
        let x = |v: i32| ((v + self.ui.content_x) as f32 * k).round() as i32;
        let y = |v: i32| (v as f32 * k).round() as i32;
        uia::to_screen(self.hwnd, (x(l), y(t), x(r), y(b)))
    }

    fn screen_client(&self, l: i32, t: i32, r: i32, b: i32) -> [f64; 4] {
        let k = self.ui.k;
        let s = |v: i32| (v as f32 * k).round() as i32;
        uia::to_screen(self.hwnd, (s(l), s(t), s(r), s(b)))
    }

    fn key(&mut self, tag: &str) -> u64 {
        let base = format!("{}|{tag}", self.ui.page.title());
        let count = self.seen.entry(base.clone()).or_insert(0);
        *count += 1;
        fnv64(&format!("{base}#{count}"))
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        tag: &str,
        role: Role,
        name: &str,
        help: &str,
        rect: [f64; 4],
        focusable: bool,
        action: Do,
        row: Option<(usize, usize)>,
    ) -> usize {
        let key = self.key(tag);
        // Only the scrolling content can be off-screen; the navigation pane and the trial bar
        // are always where they are.
        let offscreen =
            row.is_some() && (rect[1] + rect[3] <= self.view.0 || rect[1] >= self.view.1);
        self.out.push(Built {
            info: Info {
                key,
                name: name.to_string(),
                help: help.to_string(),
                role,
                rect,
                enabled: true,
                focusable,
                focused: false,
                offscreen,
                toggled: None,
                selected: None,
                value: None,
                automation_id: format!("{key:016x}"),
            },
            action,
            row,
        });
        self.out.len() - 1
    }
}

impl Ui {
    fn uia_build(&self, hwnd: HWND) -> Vec<Built> {
        let _measure = MeasureScope::new(self, None, self.lw(hwnd));
        let view = {
            let (_, client_h) = client_size(hwnd);
            let top = self.px(self.top());
            let bottom = client_h - self.px(self.bottom_h());
            let rect = uia::to_screen(hwnd, (0, top, 0, bottom));
            (rect[1], rect[1] + rect[3])
        };
        let mut b = Builder2 {
            ui: self,
            hwnd,
            out: Vec::new(),
            seen: HashMap::new(),
            view,
        };
        // Navigation pane first: it is the first Tab stop.
        if self.nav_w > 0 {
            // The search box is a native edit with its own accessible name; the narrow rail shows
            // a button instead.
            if self.nav_w <= crate::ui_metrics::NAV_COMPACT {
                let (l, t, r, bm) = pages::nav_search_rect(self);
                b.push(
                    "nav:search",
                    Role::Button,
                    "Search",
                    "Search settings, actions, scenes and help",
                    b.screen_client(l, t, r, bm),
                    true,
                    Do::FocusSearch,
                    None,
                );
            }
            for (index, category) in crate::Category::ALL.iter().enumerate() {
                let (l, t, r, bm) = pages::nav_item_rect(self, index);
                let i = b.push(
                    &format!("nav:{}", category.title()),
                    Role::TabItem,
                    category.title(),
                    category.landing().description(),
                    b.screen_client(l, t, r, bm),
                    true,
                    Do::Nav(category.landing()),
                    None,
                );
                b.out[i].info.selected = Some(*category == self.page.category());
            }
        }
        let rows = self.rows();
        let lw = self.lw(hwnd);
        let pinned = self.top();
        for (row_index, row) in rows.iter().enumerate() {
            let top = row.y - self.scroll + pinned;
            let (cl, cr) = (MARGIN, lw - MARGIN);
            let card = |b: &Builder2| b.screen(cl, top, cr, top + row.h - GAP);
            match &row.row {
                Row::Header(text) => {
                    b.push(
                        &format!("h:{text}"),
                        Role::Heading,
                        text,
                        "",
                        b.screen(cl, top + 10, cr, top + 40),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::PageTitle(title, description) => {
                    b.push(
                        "title",
                        Role::Heading,
                        title,
                        description,
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::Brand => {
                    b.push(
                        "brand",
                        Role::Heading,
                        crate::BRAND_NAME,
                        crate::BRAND_TAGLINE,
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::Credit => {
                    let layout = credit_layout(self.lw(hwnd));
                    let (pl, pt, pr, pb) = layout.prefix;
                    b.push(
                        "credit:text",
                        Role::Text,
                        crate::CREDIT_PREFIX.trim_end(),
                        "",
                        b.screen(pl, top + pt, pr, top + pb),
                        false,
                        Do::Nothing,
                        None,
                    );
                    let (ll, lt, lr, lb) = layout.link;
                    let i = b.push(
                        "credit:link",
                        Role::Link,
                        crate::CREDIT_LINK_NAME,
                        "Opens in your browser",
                        b.screen(ll, top + lt, lr, top + lb),
                        true,
                        Do::Credit,
                        Some((row_index, 0)),
                    );
                    b.out[i].info.value = Some(crate::CREDIT_URL.to_string());
                }
                Row::Note(text) => {
                    b.push(
                        "note",
                        Role::Text,
                        text,
                        "",
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::Tip(parts) => {
                    b.push(
                        "tip",
                        Role::Text,
                        &parts.join(". "),
                        "",
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::Lines(lines) | Row::About(lines) | Row::Prose(lines) => {
                    b.push(
                        "text",
                        Role::Text,
                        &lines.join(" "),
                        "",
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::Crumb => {
                    let parent = self.page.parent().map(|p| p.title()).unwrap_or("");
                    b.push(
                        "crumb:back",
                        Role::Button,
                        &format!("Back to {parent}"),
                        "Alt+Left also goes back",
                        b.screen(cl, top, cr, top + row.h - GAP),
                        true,
                        Do::Go(self.page.parent().unwrap_or(Page::Home)),
                        Some((row_index, 0)),
                    );
                }
                Row::Link(dest) => {
                    b.push(
                        &format!("link:{}", dest.id()),
                        Role::Button,
                        dest.title(),
                        dest.description(),
                        card(&b),
                        true,
                        Do::Go(*dest),
                        Some((row_index, 0)),
                    );
                }
                Row::Result(index) => {
                    if let Some(result) = self.ext.results.get(*index) {
                        let place = if result.path.is_empty() {
                            crate::palette_destination(&result.action).0.path()
                        } else {
                            result.path.clone()
                        };
                        b.push(
                            &format!("result:{index}"),
                            Role::Button,
                            &format!("{}, {}", result.title, place),
                            &result.hint,
                            card(&b),
                            true,
                            Do::Result(*index),
                            Some((row_index, 0)),
                        );
                    }
                }
                Row::Expander(title) => {
                    let i = b.push(
                        &format!("section:{title}"),
                        Role::Toggle,
                        title,
                        "Show or hide the options in this section",
                        card(&b),
                        true,
                        Do::Section(title),
                        Some((row_index, 0)),
                    );
                    b.out[i].info.toggled = Some(self.section_open(title));
                }
                Row::GlobalSearch => {
                    // The native search box is the control; the card around it is decoration.
                }
                Row::Toggle(id) => {
                    let (label, desc) = toggle_meta(*id);
                    let i = b.push(
                        &format!("toggle:{label}"),
                        Role::Toggle,
                        label,
                        desc,
                        card(&b),
                        true,
                        Do::Toggle(*id),
                        Some((row_index, 0)),
                    );
                    b.out[i].info.toggled = Some(toggle_on(self, *id));
                }
                Row::Choice(id) => {
                    let i = b.push(
                        &format!("choice:{}", choice_title(*id)),
                        Role::Button,
                        choice_title(*id),
                        choice_desc(self, *id),
                        card(&b),
                        true,
                        Do::Choice(*id),
                        Some((row_index, 0)),
                    );
                    b.out[i].info.value = Some(choice_text(self, *id));
                }
                Row::Slider(id) => {
                    let (label, desc) = slider_meta(*id);
                    let (_, value) = slider_state(self, *id);
                    let i = b.push(
                        &format!("slider:{label}"),
                        Role::Slider,
                        label,
                        desc,
                        card(&b),
                        true,
                        Do::Nothing,
                        Some((row_index, 0)),
                    );
                    b.out[i].info.value = Some(value);
                }
                Row::Color(id) => {
                    b.push(
                        &format!("color:{id}"),
                        Role::Text,
                        color_label(*id),
                        "Type a color in the box beside this label.",
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::Presets => {
                    for (index, preset) in PRESETS.iter().enumerate() {
                        let (l, t, r, bm) = preset_rect(lw, index);
                        b.push(
                            &format!("preset:{}", preset.name),
                            Role::Button,
                            preset.name,
                            preset.blurb,
                            b.screen(l, top + t, r, top + bm),
                            true,
                            Do::Preset(index),
                            Some((row_index, index)),
                        );
                    }
                }
                Row::Action(id) => {
                    let (title, desc, label) = button_meta(*id);
                    let name = if title.is_empty() { label } else { title };
                    b.push(
                        &format!("button:{name}"),
                        Role::Button,
                        name,
                        desc,
                        card(&b),
                        true,
                        Do::Button(*id),
                        Some((row_index, 0)),
                    );
                }
                Row::AddField(id) => {
                    let (_, _, label) = button_meta(*id);
                    let (l, t, r, bm) = add_pill(lw);
                    b.push(
                        &format!("button:{label}"),
                        Role::Button,
                        label,
                        "",
                        b.screen(l, top + t, r, top + bm),
                        true,
                        Do::Button(*id),
                        Some((row_index, 0)),
                    );
                }
                Row::Buttons(list) => {
                    for (index, id) in list.iter().enumerate() {
                        let (_, _, label) = button_meta(*id);
                        let (l, t, r, bm) = buttons_rect(lw, list.len(), index, row.h);
                        b.push(
                            &format!("button:{label}"),
                            Role::Button,
                            label,
                            "",
                            b.screen(l, top + t, r, top + bm),
                            true,
                            Do::Button(*id),
                            Some((row_index, index)),
                        );
                    }
                }
                Row::Filter => {
                    b.push(
                        "filter",
                        Role::Text,
                        "Filter the list. Type in the box beside this label.",
                        "",
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::SearchField => {
                    b.push(
                        "search",
                        Role::Text,
                        "Search help. Type in the box beside this label.",
                        "",
                        card(&b),
                        false,
                        Do::Nothing,
                        None,
                    );
                }
                Row::WindowList => {
                    for (index, entry) in self.visible_windows().iter().enumerate().take(40) {
                        let line = self.window_line(entry);
                        let t = top + 8 + index as i32 * LIST_LINE;
                        let selected = self
                            .ext
                            .selected
                            .as_ref()
                            .is_some_and(|s| s.key == entry.key && s.pid == entry.pid);
                        let i = b.push(
                            &format!("window:{line}"),
                            Role::ListItem,
                            &line,
                            "",
                            b.screen(cl, t, cr, t + LIST_LINE),
                            true,
                            Do::Window(index),
                            Some((row_index, index)),
                        );
                        b.out[i].info.selected = Some(selected);
                    }
                }
                Row::WallGrid => {
                    for (index, entry) in self.ext.walls.iter().enumerate() {
                        let (l, t, r, bt) = pages::wall_rect_pub(lw, index);
                        let status = crate::wallpaper::status_text(
                            entry,
                            crate::platform::gallery::downloaded(entry),
                            &self.ext.wall_state,
                        );
                        let i = b.push(
                            &format!("wallpaper:{}", entry.id),
                            Role::ListItem,
                            &format!("{}, {}", entry.name, status),
                            &entry.description,
                            b.screen(l, top + t, r, top + bt),
                            true,
                            Do::Wall(index),
                            Some((row_index, index)),
                        );
                        b.out[i].info.selected = Some(self.ext.wall_sel == index);
                    }
                }
                Row::RecipeList => {
                    for (index, (recipe, _)) in self.ext.recipes.iter().enumerate().take(40) {
                        let t = top + 8 + index as i32 * LIST_LINE;
                        let i = b.push(
                            &format!("recipe:{}", recipe.id),
                            Role::ListItem,
                            &recipe.name,
                            "",
                            b.screen(cl, t, cr, t + LIST_LINE),
                            true,
                            Do::Recipe(index),
                            Some((row_index, index)),
                        );
                        b.out[i].info.selected = Some(self.ext.recipe_sel == Some(index));
                    }
                }
                Row::LookList => {
                    for index in 0..self.look_count().min(40) {
                        let name = self.look_name(index);
                        let t = top + 8 + index as i32 * LIST_LINE;
                        let i = b.push(
                            &format!("look:{name}"),
                            Role::ListItem,
                            &name,
                            "",
                            b.screen(cl, t, cr, t + LIST_LINE),
                            true,
                            Do::Look(index),
                            Some((row_index, index)),
                        );
                        b.out[i].info.selected = Some(self.ext.look_sel == Some(index));
                    }
                }
                Row::RuleList => {
                    for (index, rule) in self.config.rules.iter().enumerate().take(40) {
                        let t = top + 8 + index as i32 * LIST_LINE;
                        let i = b.push(
                            &format!("rule:{index}"),
                            Role::ListItem,
                            &rule.label(),
                            "",
                            b.screen(cl, t, cr, t + LIST_LINE),
                            true,
                            Do::Rule(index),
                            Some((row_index, index)),
                        );
                        b.out[i].info.selected = Some(self.rule == Some(index));
                    }
                }
                Row::ExcludeList => {
                    for (index, name) in self.config.exclude.iter().enumerate().take(60) {
                        let t = top + 8 + index as i32 * LIST_LINE;
                        let i = b.push(
                            &format!("exclude:{index}"),
                            Role::ListItem,
                            name,
                            "",
                            b.screen(cl, t, cr, t + LIST_LINE),
                            true,
                            Do::Exclude(index),
                            Some((row_index, index)),
                        );
                        b.out[i].info.selected = Some(self.exclude_sel == Some(index));
                    }
                }
            }
        }
        // The trial bar: what is being tried, and the two ways to answer it. Keep and Revert come
        // after the page, matching the Tab order.
        if self.trial_active() {
            let (keep, revert) = pages::trial_pills(lw, self.status_top());
            let message = self.trial_message();
            b.push(
                "trial:message",
                Role::Text,
                &message,
                "",
                b.screen(MARGIN, keep.1, lw - MARGIN - 200, keep.3),
                false,
                Do::Nothing,
                None,
            );
            b.push(
                "trial:keep",
                Role::Button,
                "Keep",
                "Saves the look being tried.",
                b.screen(keep.0, keep.1, keep.2, keep.3),
                true,
                Do::TrialKeep,
                None,
            );
            b.push(
                "trial:revert",
                Role::Button,
                "Revert",
                "Puts back the look from before the trial.",
                b.screen(revert.0, revert.1, revert.2, revert.3),
                true,
                Do::TrialRevert,
                None,
            );
        }
        // The status message, so a screen reader can read what just happened.
        let status = if self.status.is_empty() {
            "Changes apply as you make them."
        } else {
            self.status.as_str()
        };
        let lh = self.client_lh;
        b.push(
            "status",
            Role::Text,
            &format!("Status: {status}"),
            "",
            b.screen_client(
                self.content_x + MARGIN,
                lh - STATUS_H,
                self.content_x + lw - MARGIN,
                lh,
            ),
            false,
            Do::Nothing,
            None,
        );

        // Which element has keyboard focus.
        let focused_key = self.uia_focused_index(&b.out);
        if let Some(index) = focused_key {
            if let Some(built) = b.out.get_mut(index) {
                built.info.focused = true;
            }
        }
        b.out
    }

    /// Position in `built` of the element holding keyboard focus, if any.
    fn uia_focused_index(&self, built: &[Built]) -> Option<usize> {
        if let Some(button) = self.trial_focus() {
            return built.iter().position(|b| match b.action {
                Do::TrialKeep => button == 0,
                Do::TrialRevert => button == 1,
                _ => false,
            });
        }
        if self.ext.nav_focus {
            return built.iter().position(
                |b| matches!(b.action, Do::Nav(page) if page.category() == self.page.category()),
            );
        }
        let ordinal = self.ext.focus?;
        let (row_index, _) = *self.interactive_rows().get(ordinal)?;
        let list: Vec<usize> = built
            .iter()
            .enumerate()
            .filter(|(_, b)| b.row.is_some_and(|(r, _)| r == row_index))
            .map(|(i, _)| i)
            .collect();
        if list.is_empty() {
            return None;
        }
        // Composite rows (a row of buttons, a list) focus one item; single controls have one.
        let sub = self.ext.focus_sub.min(list.len() - 1);
        list.get(sub).copied()
    }

    /// The key of the focused element, only computed when a client is listening.
    pub fn uia_focused_key(&self, hwnd: HWND) -> Option<u64> {
        let built = self.uia_build(hwnd);
        let index = self.uia_focused_index(&built)?;
        built.get(index).map(|b| b.info.key)
    }

    /// Tell a listening screen reader that focus moved. Does nothing when no client is listening.
    pub fn uia_announce_focus(&self, hwnd: HWND) {
        if !uia::listening() {
            return;
        }
        if let Some(key) = self.uia_focused_key(hwnd) {
            unsafe { uia::raise_focus(hwnd, key) };
        }
    }

    /// Answer a provider's question on the UI thread.
    pub fn uia_answer(&mut self, hwnd: HWND, query: &mut Query) {
        match query.op {
            Op::Snapshot => {
                query.all = self.uia_build(hwnd).into_iter().map(|b| b.info).collect();
                query.ok = true;
            }
            Op::Act => query.ok = self.uia_act(hwnd, query.key),
            Op::Focus => query.ok = self.uia_focus(hwnd, query.key),
        }
    }

    fn uia_act(&mut self, hwnd: HWND, key: u64) -> bool {
        let built = self.uia_build(hwnd);
        let Some(found) = built.iter().find(|b| b.info.key == key) else {
            return false;
        };
        let action = found.action;
        // While a trial runs, the same rule as a click: finish it first.
        if self.trial_active() && !matches!(action, Do::Nothing | Do::TrialKeep | Do::TrialRevert) {
            self.status = "Keep or revert the trial look first.".into();
            return false;
        }
        match action {
            Do::Nothing => return false,
            Do::Nav(page) => {
                self.ext.nav_focus = true;
                self.set_page(hwnd, page);
            }
            Do::Toggle(id) => self.toggle(id),
            Do::Choice(id) => self.cycle(id),
            Do::Button(id) => self.button(id),
            Do::Preset(index) => self.apply_preset(index),
            Do::Window(index) => self.click_window_list(index as i32),
            Do::Recipe(index) => self.click_recipe_list(index as i32),
            Do::Look(index) => self.click_look_list(index as i32),
            Do::Wall(index) => self.select_wall(index),
            Do::Rule(index) => self.pick_rule(index as i32),
            Do::Exclude(index) => self.pick_exclude(index as i32),
            Do::TrialKeep => self.trial_keep(),
            Do::TrialRevert => self.trial_revert(),
            Do::Credit => self.open_credit(),
            Do::FocusSearch => self.focus_search(hwnd),
            Do::Go(page) => self.set_page(hwnd, page),
            Do::Result(index) => self.open_result(hwnd, index),
            Do::Section(title) => self.toggle_section(title),
        }
        self.relayout(hwnd);
        true
    }

    fn uia_focus(&mut self, hwnd: HWND, key: u64) -> bool {
        let built = self.uia_build(hwnd);
        let Some(found) = built.iter().find(|b| b.info.key == key) else {
            return false;
        };
        match (found.action, found.row) {
            (Do::Nav(page), _) => {
                self.ext.nav_focus = true;
                self.ext.focus = None;
                self.set_page(hwnd, page);
            }
            (Do::TrialKeep | Do::TrialRevert, _) => {
                self.ext.nav_focus = false;
                self.ext.focus = None;
                self.ext.trial_focus = Some(u8::from(matches!(found.action, Do::TrialRevert)));
            }
            (_, Some((row_index, sub))) => {
                let Some(ordinal) = self
                    .interactive_rows()
                    .iter()
                    .position(|(r, _)| *r == row_index)
                else {
                    return false;
                };
                self.ext.nav_focus = false;
                self.ext.focus = Some(ordinal);
                self.ext.focus_sub = sub;
                self.scroll_focus_into_view_pub(hwnd);
            }
            _ => return false,
        }
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
        true
    }
}
