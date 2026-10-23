//! Rendering. Colors are ANSI palette entries on purpose: herdr runs with `theme = "terminal"`,
//! so the panel follows whatever palette the outer terminal uses.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::app::{App, Mode, Place, Row};
use crate::data::now;

const SPIN: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Herdr's Tokyo Night Day selection background; darker than white while preserving text contrast.
const SELECTION_BG: Color = Color::Rgb(183, 193, 227);

struct Icons {
    repo: &'static str,
    pr: &'static str,
    comment: &'static str,
}

fn icons() -> Icons {
    if std::env::var("WORKTREES_ICONS").as_deref() == Ok("plain") {
        Icons { repo: "⎇", pr: "#", comment: "c" }
    } else {
        Icons { repo: "\u{e0a0}", pr: "\u{f407} ", comment: "\u{f075} " }
    }
}

pub fn render(f: &mut Frame, app: &mut App) {
    let foot = footer(app, f.area().width);
    let [head, body, foot_area] =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(foot.len() as u16)])
            .areas(f.area());
    header(f, app, head);
    list(f, app, body);
    f.render_widget(Paragraph::new(foot), foot_area);
}

fn spread(left: Vec<Span<'static>>, right: Vec<Span<'static>>, width: u16) -> Line<'static> {
    let lw: usize = left.iter().map(Span::width).sum();
    let rw: usize = right.iter().map(Span::width).sum();
    let pad = (width as usize).saturating_sub(lw + rw).max(1);
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(pad)));
    spans.extend(right);
    Line::from(spans)
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{keep}…")
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = vec![String::new()];
    let mut used = 0;
    for ch in s.chars() {
        let ch_width = Span::raw(ch.to_string()).width();
        if used > 0 && used + ch_width > width {
            lines.push(String::new());
            used = 0;
        }
        lines.last_mut().expect("line exists").push(ch);
        used += ch_width;
    }
    lines
}

fn rule(width: u16) -> Line<'static> {
    Line::from("─".repeat(width as usize)).fg(Color::DarkGray)
}

fn header(f: &mut Frame, app: &App, area: Rect) {
    let ic = icons();
    let w = area.width;
    let title = spread(
        vec![" ".into(), ic.repo.fg(Color::Cyan).bold(), " ".into(), app.repo.clone().bold()],
        vec![format!("vs {} ", app.base).fg(Color::DarkGray)],
        w,
    );
    let count = |pred: fn(&Row) -> bool| app.rows.iter().filter(|r| pred(r)).count();
    let (blocked, working, done, failed) = (
        count(|r| r.top_status() == "blocked"),
        count(|r| r.top_status() == "working"),
        count(|r| r.top_status() == "done"),
        count(Row::ci_failed),
    );
    let n = app.rows.len();
    let mut left: Vec<Span> = vec![format!(" {n} worktree{}", if n == 1 { "" } else { "s" }).fg(Color::DarkGray)];
    let mut badge = |n: usize, glyph: &str, color: Color| {
        if n > 0 {
            left.push(format!("  {glyph}{n}").fg(color).bold());
        }
    };
    badge(blocked, "◉", Color::Red);
    badge(working, SPIN[app.frame() % 10], Color::Yellow);
    badge(done, "✓", Color::Green);
    badge(failed, "✗", Color::Red);
    let sync = if app.pr_at == 0 { "⟳ –".to_string() } else { format!("⟳ {} ", age(app.pr_at)) };
    let summary = spread(left, vec![sync.fg(Color::DarkGray)], w);
    f.render_widget(Paragraph::new(vec![title, summary, rule(w)]), area);
}

fn age(ts: i64) -> String {
    let s = now() - ts;
    if ts == 0 {
        String::new()
    } else if s < 60 {
        "now".into()
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else if s < 86400 {
        format!("{}h", s / 3600)
    } else {
        format!("{}d", s / 86400)
    }
}

/// Heat by recency: fresh work stands out, stale checkouts fade.
fn age_color(ts: i64) -> Color {
    let s = now() - ts;
    if s < 3600 {
        Color::Green
    } else if s < 86400 {
        Color::Reset
    } else if s < 7 * 86400 {
        Color::Gray
    } else {
        Color::DarkGray
    }
}

/// The card's signal color: what, if anything, this worktree needs from you.
fn signal(app: &App, r: &Row) -> Style {
    let base = match r.top_status() {
        "blocked" => Color::Red,
        _ if r.ci_failed() => Color::Red,
        "working" => Color::Yellow,
        "done" => Color::Green,
        _ => Color::DarkGray,
    };
    if app.pulse_on(&r.path) {
        Style::new().fg(Color::LightRed).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(base)
    }
}

fn card(app: &App, r: &Row, w: u16, sel: bool, hover: bool) -> Vec<Line<'static>> {
    let ic = icons();
    let sig = signal(app, r);
    let bar = || Span::styled("▌", sig);
    let spin = SPIN[app.frame() % 10];
    let g = r.git.clone().unwrap_or_default();

    // Line 1: place, branch, dirty / ahead / behind.
    let mark = match r.place {
        Place::Here => "◆".fg(Color::Cyan),
        Place::Open => "◇".fg(Color::Cyan),
        Place::Prunable => "✗".fg(Color::Red),
        Place::Closed => " ".into(),
    };
    let mut right: Vec<Span> = Vec::new();
    if g.dirty > 0 {
        right.push(format!("✎{} ", g.dirty).fg(Color::Yellow));
    }
    if g.ahead > 0 {
        right.push(format!("↑{}", g.ahead).fg(Color::Green));
    }
    if g.behind > 0 {
        let c = match g.behind {
            50.. => Color::Red,
            10.. => Color::Yellow,
            _ => Color::DarkGray,
        };
        right.push(format!("↓{}", g.behind).fg(c));
    }
    right.push(" ".into());
    // Reserve one cell for `spread`'s gap, then continue the complete branch below if needed.
    let room = (w as usize).saturating_sub(5 + right.iter().map(Span::width).sum::<usize>());
    let mut branch_style = Style::new();
    if hover {
        branch_style = branch_style.add_modifier(Modifier::UNDERLINED);
    }
    let mut first = vec![bar(), " ".into(), mark, " ".into()];
    let mut lines = Vec::new();
    if Span::raw(r.branch.clone()).width() <= room {
        first.push(Span::styled(r.branch.clone(), branch_style));
    }
    lines.push(spread(first, right, w));
    if Span::raw(r.branch.clone()).width() > room {
        for part in wrap(&r.branch, (w as usize).saturating_sub(4)) {
            lines.push(Line::from(vec![bar(), "   ".into(), Span::styled(part, branch_style)]));
        }
    }

    // Agent lines: one per agent, most urgent first, each naming which coding agent it is.
    // The first line also carries the last-commit age.
    let age_span = || format!("{} ", age(g.last_ct)).fg(age_color(g.last_ct));
    if r.agents.is_empty() {
        lines.push(spread(vec![bar(), "   ".into(), "· no agent".fg(Color::DarkGray)], vec![age_span()], w));
    }
    for (i, a) in r.agents.iter().take(MAX_AGENT_LINES).enumerate() {
        let right = if i == 0 { vec![age_span()] } else { Vec::new() };
        let rw: usize = right.iter().map(Span::width).sum();
        lines.push(spread(agent_line(a, spin, w as usize - rw, bar()), right, w));
    }
    if r.agents.len() > MAX_AGENT_LINES {
        let more = format!("+{} more", r.agents.len() - MAX_AGENT_LINES);
        lines.push(Line::from(vec![bar(), "   ".into(), more.fg(Color::DarkGray)]));
    }

    // Line 3: PR, CI, unresolved threads.
    let mut left = vec![bar(), "   ".into()];
    match &r.pr {
        Some(pr) => {
            left.push(format!("{}{}", ic.pr, pr.n).fg(Color::Magenta));
            left.push(" ".into());
            left.push(match pr.ci.as_str() {
                "SUCCESS" => "✓".fg(Color::Green),
                "FAILURE" | "ERROR" if !pr.fails.is_empty() => format!("✗{}", pr.fails.len()).fg(Color::Red).bold(),
                "FAILURE" | "ERROR" => "✗".fg(Color::Red).bold(),
                "PENDING" | "EXPECTED" => format!("{spin}{}", pr.pending).fg(Color::Yellow),
                _ => "–".fg(Color::DarkGray),
            });
            if pr.t > 0 {
                left.push(format!(" {}{}", ic.comment, pr.t).fg(Color::Yellow));
            }
            match pr.review.as_str() {
                "APPROVED" => left.push(" ✔".fg(Color::Green)),
                "CHANGES_REQUESTED" => left.push(" ±".fg(Color::Red)),
                _ => {}
            }
            if pr.draft {
                left.push(" draft".fg(Color::DarkGray));
            }
        }
        None => left.push("no PR".fg(Color::DarkGray)),
    }
    lines.push(Line::from(left));

    if sel && app.expanded {
        lines.extend(details(app, r, &g, w));
    }
    lines
}

/// Agents listed per card before collapsing the rest into "+N more".
const MAX_AGENT_LINES: usize = 3;

/// `⠧ claude · what it is doing`: state glyph, agent name in bold, then the pane title.
fn agent_line(a: &crate::data::Agent, spin: &'static str, width: usize, bar: Span<'static>) -> Vec<Span<'static>> {
    let (glyph, state, style) = match a.status.as_str() {
        "blocked" => ("◉", "needs you", Style::new().fg(Color::Red).bold()),
        "working" => (spin, "", Style::new().fg(Color::Yellow)),
        "done" => ("✓", "done", Style::new().fg(Color::Green)),
        "idle" => ("●", "", Style::new().fg(Color::Gray)),
        _ => ("○", "", Style::new().fg(Color::DarkGray)),
    };
    let name_style = Style::new().fg(Color::DarkGray).bold();
    let label_style = Style::new().fg(Color::DarkGray);
    // bar + 3 spaces + glyph + space + name + " · " + 1 cell of breathing room before the right column.
    let room = width.saturating_sub(4 + 2 + a.name.chars().count() + 3 + 1);
    let mut spans =
        vec![bar, "   ".into(), Span::styled(format!("{glyph} "), style), Span::styled(a.name.clone(), name_style)];
    if (!state.is_empty() || !a.title.is_empty()) && room > 1 {
        spans.push(" · ".fg(Color::DarkGray));
        if !state.is_empty() {
            spans.push(Span::styled(state, style));
        }
        let title_room = room.saturating_sub(Span::raw(state).width() + if state.is_empty() { 0 } else { 3 });
        if !a.title.is_empty() && title_room > 1 {
            if !state.is_empty() {
                spans.push(" · ".fg(Color::DarkGray));
            }
            spans.push(Span::styled(cut(&a.title, title_room), label_style));
        }
    }
    spans
}

fn details(app: &App, r: &Row, g: &crate::data::GitInfo, w: u16) -> Vec<Line<'static>> {
    let room = (w as usize).saturating_sub(5);
    let pad = || Span::raw("    ");
    let mut out = vec![Line::from("")];
    if !g.subject.is_empty() {
        out.push(Line::from(vec![pad(), format!("“{}”", cut(&g.subject, room - 2)).italic().fg(Color::DarkGray)]));
    }
    if g.ahead > 0 {
        let total = (g.ins + g.del).max(1);
        let green = (10 * g.ins).div_ceil(total).min(10) as usize;
        out.push(Line::from(vec![
            pad(),
            "▇".repeat(green).fg(Color::Green),
            "▇".repeat(10 - green).fg(Color::Red),
            format!(" +{} −{} · {}f", g.ins, g.del, g.files).fg(Color::DarkGray),
        ]));
    } else {
        out.push(Line::from(vec![pad(), format!("no commits ahead of {}", app.base).fg(Color::DarkGray)]));
    }
    if let Some(pr) = &r.pr {
        if !pr.fails.is_empty() {
            out.push(Line::from(vec![pad(), format!("✗ {}", cut(&pr.fails.join(", "), room - 2)).fg(Color::Red)]));
        }
    }
    let shown = checkout_path(&r.path, &app.root);
    out.push(Line::from(vec![pad(), cut(&shown, room).fg(Color::DarkGray)]));
    out
}

fn checkout_path(path: &str, root: &str) -> String {
    if let Some(relative) = path.strip_prefix(root) {
        format!(".{relative}")
    } else if let Some((_, relative)) = path.split_once("/.herdr/worktrees/") {
        relative.to_string()
    } else {
        path.to_string()
    }
}

fn list(f: &mut Frame, app: &mut App, area: Rect) {
    app.hits.clear();
    if app.rows.is_empty() {
        let msg = match (&app.snap.error, app.filter.is_empty()) {
            (Some(e), _) => e.clone(),
            (None, false) => format!("no worktree matches “{}”", app.filter),
            (None, true) => "loading…".into(),
        };
        f.render_widget(
            Paragraph::new(msg).fg(Color::DarkGray).centered(),
            Rect { y: area.y + 1, height: area.height.saturating_sub(1), ..area },
        );
        return;
    }
    let w = area.width;
    let rows = app.rows.clone();
    let hover = app.hover.clone();
    // Selection changes color only, so moving it never shifts or resizes cards.
    let mut cards = Vec::new();
    let mut y = 0u16;
    for r in &rows {
        let sel = app.selected.as_deref() == Some(r.path.as_str());
        let lines = card(app, r, w, sel, hover.as_deref() == Some(r.path.as_str()));
        let h = lines.len() as u16 + 1;
        cards.push((r.path.clone(), sel, lines, y, h));
        y += h;
    }
    // Keep the selected card fully visible.
    if let Some((_, _, _, sy, sh)) = cards.iter().find(|c| c.1) {
        if *sy < app.scroll {
            app.scroll = *sy;
        } else if sy + sh > app.scroll + area.height {
            app.scroll = (sy + sh).saturating_sub(area.height);
        }
    }
    app.scroll = app.scroll.min(y.saturating_sub(area.height));

    // Draw in order of animated position so a card sliding over another stays on top.
    let mut placed: Vec<_> = cards
        .into_iter()
        .map(|(path, sel, lines, ty, h)| {
            let dy = app.slide_y(&path, ty as f32);
            (path, sel, lines, dy, h)
        })
        .collect();
    placed.sort_by(|a, b| a.3.total_cmp(&b.3));
    for (path, sel, lines, dy, h) in placed {
        let top = area.y as i32 + dy.round() as i32 - app.scroll as i32;
        let bottom = top + h as i32;
        let (vis_top, vis_bottom) = (top.max(area.y as i32), bottom.min(area.bottom() as i32));
        if vis_top >= vis_bottom {
            continue;
        }
        let rect = Rect::new(area.x, vis_top as u16, w, (vis_bottom - vis_top) as u16);
        let skip = (vis_top - top) as u16;
        f.render_widget(Clear, rect);
        let para = Paragraph::new(lines).scroll((skip, 0));
        let para = if sel { para.style(Style::new().bg(SELECTION_BG)) } else { para };
        f.render_widget(para, rect);
        app.hits.push((rect, path));
    }
}

fn hints(pairs: &[(&str, &str)], width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![Vec::<Span>::new()];
    let mut used = 0usize;
    for (k, label) in pairs {
        let len = k.chars().count() + label.chars().count() + 3;
        if used + len > width as usize && used > 0 {
            lines.push(Vec::new());
            used = 0;
        }
        let cur = lines.last_mut().expect("non-empty");
        cur.push(format!(" {k}").fg(Color::Cyan));
        cur.push(format!(" {label} ").fg(Color::DarkGray));
        used += len;
    }
    lines.into_iter().map(Line::from).collect()
}

fn footer(app: &App, w: u16) -> Vec<Line<'static>> {
    let cursor = || "▏".fg(Color::Cyan);
    let (prompt, keys): (Line, Vec<(&str, &str)>) = match &app.mode {
        Mode::Filter => (
            Line::from(vec![" / ".fg(Color::Cyan), app.filter.clone().into(), cursor()]),
            vec![("↵", "keep"), ("esc", "clear")],
        ),
        Mode::NewBranch(s) => (
            Line::from(vec![" new branch ".fg(Color::Cyan), s.clone().into(), cursor()]),
            vec![("↵", "create"), ("esc", "cancel")],
        ),
        Mode::ConfirmDelete(p) => {
            let b = app.rows.iter().find(|r| &r.path == p).map(|r| r.branch.clone()).unwrap_or_default();
            (
                Line::from(format!(" remove {}? ", cut(&b, w as usize - 14))).fg(Color::Red).bold(),
                vec![("y", "yes"), ("n", "no")],
            )
        }
        Mode::Normal => {
            let status = app.status.as_ref().filter(|(_, _, at)| at.elapsed().as_secs() < 5);
            let prompt = match status {
                Some((t, ok, _)) => {
                    Line::from(format!(" {}", cut(t, w as usize - 2))).fg(if *ok { Color::Green } else { Color::Red })
                }
                None if !app.filter.is_empty() => Line::from(vec![" / ".fg(Color::Cyan), app.filter.clone().into()]),
                None => Line::from(""),
            };
            (
                prompt,
                vec![
                    ("↵", "open"),
                    ("⇥", "info"),
                    ("n", "new"),
                    ("d", "del"),
                    ("o", "PR"),
                    ("y", "copy"),
                    ("/", "find"),
                    ("q", "hide"),
                ],
            )
        }
    };
    let mut lines = vec![rule(w), prompt];
    lines.extend(hints(&keys, w));
    lines
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;
    use ratatui::Terminal;

    use crate::app::App;
    use crate::data::{Agent, HerdrSnap, Msg, WtRaw};

    fn render_buffer(branch: &str, agents: Vec<(&str, &str, &str)>, width: u16) -> Buffer {
        let (tx, _rx) = channel();
        let kicks = (0..3).map(|_| channel().0).collect();
        let mut app = App::new(
            "w1".into(),
            String::new(),
            "repo".into(),
            "/repo".into(),
            "origin/main".into(),
            String::new(),
            Arc::new(Mutex::new(Vec::new())),
            tx,
            kicks,
        );
        let wt = |path: &str, branch: &str| WtRaw {
            path: path.into(),
            branch: branch.into(),
            prunable: false,
            linked: path != "/repo",
            open_ws: None,
        };
        app.apply(Msg::Herdr(HerdrSnap {
            worktrees: vec![wt("/repo", "main"), wt("/repo/.worktrees/feat", branch)],
            agents: agents
                .into_iter()
                .map(|(name, status, title)| {
                    let a = Agent { status: status.into(), name: name.into(), title: title.into() };
                    ("/repo/.worktrees/feat".to_string(), a)
                })
                .collect(),
            error: None,
        }));
        let Ok(mut term) = Terminal::new(TestBackend::new(width, 30));
        let Ok(_) = term.draw(|f| super::render(f, &mut app));
        term.backend().buffer().clone()
    }

    fn render(agents: Vec<(&str, &str, &str)>) -> Vec<String> {
        let buf = render_buffer("feat", agents, 38);
        (0..30).map(|y| (0..38).map(|x| buf[(x, y)].symbol().to_string()).collect()).collect()
    }

    fn text_cell(buf: &Buffer, needle: &str) -> (u16, u16) {
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(x) = line.find(needle) {
                return (line[..x].chars().count() as u16, y);
            }
        }
        panic!("{needle:?} not rendered")
    }

    #[test]
    fn every_agent_in_a_worktree_is_named_most_urgent_first() {
        let lines = render(vec![("claude", "working", "Fix login"), ("codex", "blocked", "Review")]);
        let at = |needle: &str| lines.iter().position(|l| l.contains(needle));
        let (codex, claude) = (at("codex").expect("codex shown"), at("claude").expect("claude shown"));
        assert!(codex < claude, "the agent waiting on you is listed first");
        assert!(lines[codex].contains("needs you"));
        assert!(lines[claude].contains("Fix login"));
    }

    #[test]
    fn agents_beyond_the_limit_collapse_into_a_count() {
        let lines =
            render(vec![("claude", "idle", ""), ("codex", "idle", ""), ("pi", "idle", ""), ("opencode", "idle", "")]);
        assert!(lines.iter().any(|l| l.contains("+1 more")));
        assert!(!lines.iter().any(|l| l.contains("opencode")));
    }

    #[test]
    fn long_branch_wraps_without_ellipsis_or_border_overflow() {
        let branch = "feature/keep-every-character-inside-the-selected-card";
        let buf = render_buffer(branch, Vec::new(), 20);
        let text = (0..buf.area.height)
            .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<String>();
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace() && *ch != '▌').collect();
        assert!(compact.contains(branch));
        assert!(!text.contains('…'));
    }

    #[test]
    fn managed_worktree_path_omits_the_private_home_prefix() {
        assert_eq!(
            super::checkout_path(
                "/any/home/.herdr/worktrees/qa-backend-tests/fix-wms-qacore-907-public-urls",
                "/any/home/projects/qa-backend-tests",
            ),
            "qa-backend-tests/fix-wms-qacore-907-public-urls"
        );
    }

    #[test]
    fn selection_uses_color_without_a_resizing_frame() {
        let buf = render_buffer("feat", Vec::new(), 38);
        let (y, line) = (0..buf.area.height)
            .map(|y| {
                let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
                (y, line)
            })
            .find(|(y, line)| *y >= 3 && line.contains("main"))
            .expect("selected main row");
        let x = line.find("main").expect("main branch") as u16;
        assert_eq!(buf[(x, y)].bg, super::SELECTION_BG);
        assert!(!line.contains('╭'));
        assert!(!line.contains('╰'));
    }

    #[test]
    fn agent_and_session_labels_are_dark_gray_but_the_glyph_keeps_its_status_color() {
        let buf = render_buffer("feat", vec![("claude", "working", "session title")], 38);
        let (name_x, name_y) = text_cell(&buf, "claude");
        let (title_x, title_y) = text_cell(&buf, "session title");
        assert_eq!(buf[(name_x, name_y)].fg, Color::DarkGray);
        assert_eq!(buf[(title_x, title_y)].fg, Color::DarkGray);
        assert_eq!(buf[(name_x - 2, name_y)].fg, Color::Yellow);
    }
}
