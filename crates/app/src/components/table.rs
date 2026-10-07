pub(crate) use crate::components::common::clipped_text;
use crate::components::style;
use gpui::prelude::*;
use gpui::*;

pub(crate) const TABLE_GAP: f32 = 16.0;
pub(crate) const ACTIONS_WIDTH: f32 = 92.0;
pub(crate) const STATUS_WIDTH: f32 = 96.0;

pub(crate) fn table_row() -> Div {
    div()
        .w_full()
        .min_w_0()
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(TABLE_GAP))
}

pub(crate) fn table_heading() -> Div {
    table_row()
        .h(gpui::rems(style::BAR / 16.0))
        .bg(rgb(style::SURFACE))
        .border_b_1()
        .border_color(rgb(style::BORDER))
        .text_size(gpui::rems(style::META / 16.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(style::TEXT_MUTED))
}

pub(crate) fn fixed_column(width: f32) -> Div {
    div().w(rems(width / 16.0)).flex_none().min_w_0()
}

/// Narrow tables keep their identity and status together, then place actions
/// on a separate line so every control remains available.
pub(crate) fn responsive_row(
    compact: bool,
    identity: impl IntoElement,
    detail: Option<AnyElement>,
    actions: impl IntoElement,
    status: impl IntoElement,
) -> Div {
    if compact {
        div()
            .w_full()
            .min_w_0()
            .px(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(TABLE_GAP))
                    .child(identity)
                    .child(status),
            )
            .child(div().w_full().min_w_0().flex().justify_end().child(actions))
    } else {
        table_row()
            .child(identity)
            .when_some(detail, |this, detail| this.child(detail))
            .child(actions)
            .child(status)
    }
}

pub(crate) fn responsive_status_column(compact: bool) -> Div {
    fixed_column(if compact { 80.0 } else { STATUS_WIDTH })
}

pub(crate) fn placeholder_card(title: &str, description: &str) -> Div {
    div()
        .w_full()
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
        .px(px(10.0))
        .py(px(9.0))
        .flex()
        .justify_between()
        .items_center()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(gpui::rems(style::TEXT / 16.0))
                        .text_color(rgb(style::TEXT_PRIMARY))
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .text_size(gpui::rems(style::META / 16.0))
                        .text_color(rgb(style::TEXT_MUTED))
                        .child(description.to_string()),
                ),
        )
}

pub(crate) fn error_panel(title: &str, error: String) -> Div {
    div()
        .bg(rgb(0x47232B))
        .border_1()
        .border_color(rgb(0x4A252C))
        .rounded(px(style::CARD_RADIUS))
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(gpui::rems(style::TEXT / 16.0))
                .text_color(rgb(0xFF9F99))
                .child(title.to_string()),
        )
        .child(
            div()
                .text_size(gpui::rems(style::TEXT / 16.0))
                .text_color(rgb(0xF2B6B2))
                .child(error),
        )
}

/// Direction and column selection are shared; feature modules own comparisons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Direction {
    Ascending,
    Descending,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sort<C> {
    pub column: C,
    pub direction: Direction,
}
impl<C: Copy + Eq> Sort<C> {
    pub fn new(column: C) -> Self {
        Self {
            column,
            direction: Direction::Ascending,
        }
    }
    pub fn select(&mut self, column: C) {
        self.direction = if self.column == column && self.direction == Direction::Ascending {
            Direction::Descending
        } else {
            Direction::Ascending
        };
        self.column = column;
    }
    pub fn indicator(&self, column: C) -> Option<Direction> {
        (self.column == column).then_some(self.direction)
    }
    pub fn order(&self, order: std::cmp::Ordering) -> std::cmp::Ordering {
        match self.direction {
            Direction::Ascending => order,
            Direction::Descending => order.reverse(),
        }
    }
}

pub(crate) fn table_card() -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(rgb(style::SURFACE))
        .border_1()
        .border_color(rgb(style::BORDER))
        .rounded(px(style::CARD_RADIUS))
}

pub(crate) fn sort_heading(
    id: impl Into<ElementId>,
    label: &str,
    direction: Option<Direction>,
) -> Stateful<Div> {
    use crate::components::common::{control_tooltip, lucide_icon};
    use lucide_icons::Icon;
    let hint = format!(
        "Sort by {} ({})",
        label.to_lowercase(),
        if direction == Some(Direction::Ascending) {
            "descending"
        } else {
            "ascending"
        }
    );
    div()
        .id(id)
        .h_full()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(5.0))
        .cursor_pointer()
        .text_color(rgb(if direction.is_some() {
            style::TEXT_SELECTED
        } else {
            style::TEXT_MUTED
        }))
        .hover(|s| s.text_color(rgb(style::TEXT_SELECTED)))
        .tooltip(move |_, cx| control_tooltip(hint.clone(), cx))
        .child(clipped_text(label.to_owned()).flex_1())
        .child(lucide_icon(
            match direction {
                Some(Direction::Ascending) => Icon::ArrowUp,
                Some(Direction::Descending) => Icon::ArrowDown,
                None => Icon::ArrowDownUp,
            },
            style::META,
        ))
}

pub(crate) fn filter_chip(
    id: impl Into<ElementId>,
    label: &str,
    count: usize,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(rems(style::CONTROL / 16.0))
        .flex_none()
        .px(px(9.0))
        .flex()
        .items_center()
        .gap(px(6.0))
        .border_1()
        .border_color(rgb(if active {
            style::CONTROL_SELECTED_BORDER
        } else {
            style::BORDER
        }))
        .bg(rgb(if active {
            style::CONTROL_SELECTED_BG
        } else {
            style::SURFACE
        }))
        .rounded(px(style::CARD_RADIUS))
        .cursor_pointer()
        .text_size(rems(style::META / 16.0))
        .text_color(rgb(if active {
            style::TEXT_SELECTED
        } else {
            style::TEXT_MUTED
        }))
        .hover(|s| {
            s.bg(rgb(style::SURFACE_HOVER))
                .text_color(rgb(style::TEXT_SELECTED))
        })
        .child(label.to_owned())
        .child(
            div()
                .text_color(rgb(if active {
                    style::TEXT_SELECTED
                } else {
                    style::TEXT_PRIMARY
                }))
                .child(count.to_string()),
        )
}

pub(crate) fn status_label(label: impl Into<SharedString>, color: Rgba) -> Div {
    div()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.0))
        .text_size(rems(style::META / 16.0))
        .text_color(color)
        .child(div().size(px(5.0)).rounded_full().bg(color))
        .child(label.into())
}

pub(crate) struct Search {
    field: Entity<crate::components::text_field::TextField>,
    _changes: Subscription,
}
impl Search {
    pub fn new(
        placeholder: &'static str,
        scroll: fn(&crate::app::Crabdash) -> ScrollHandle,
        cx: &mut Context<crate::app::Crabdash>,
    ) -> Self {
        let field = cx.new(|cx| {
            crate::components::text_field::TextField::new("", placeholder, 0, cx).compact()
        });
        let mut previous_query = String::new();
        let changes = cx.observe(&field, move |this, field, cx| {
            let query = field.read(cx).text().trim().to_lowercase();
            if query != previous_query {
                previous_query = query;
                scroll(this).set_offset(point(px(0.0), px(0.0)));
                cx.notify();
            }
        });
        Self {
            field,
            _changes: changes,
        }
    }
    pub fn query(&self, cx: &App) -> String {
        self.field.read(cx).text().trim().to_lowercase()
    }
    pub fn render(&self) -> Div {
        div()
            .w(rems(176.0 / 16.0))
            .flex_none()
            .child(self.field.clone())
    }
}

pub(crate) fn toolbar(filters: impl IntoElement, search: &Search, compact: bool) -> Div {
    div()
        .w_full()
        .min_h(rems(style::CONTROL / 16.0))
        .flex()
        .when(compact, |this| this.flex_col())
        .items_center()
        .gap(px(if compact { 8.0 } else { 12.0 }))
        .child(
            div()
                .id("table-filters")
                .min_w_0()
                .when(compact, |this| this.w_full())
                .when(!compact, |this| this.flex_1().overflow_x_scroll())
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(filters),
        )
        .child(if compact {
            div().w_full().child(search.field.clone())
        } else {
            search.render()
        })
}
