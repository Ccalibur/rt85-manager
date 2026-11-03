use super::Ctx;
use crate::{fl, worker::DeviceSummary};
use cosmic::{Element, widget};

#[derive(Debug, Clone)]
pub enum Message {
    Select(usize),
    Rescan,
}

pub fn view<'a>(devices: &'a [DeviceSummary], active: Option<usize>, ctx: &Ctx, scan_error: Option<&'a str>) -> Element<'a, Message> {
    let space = cosmic::theme::spacing();
    let mut col: Vec<Element<Message>> = Vec::new();

    if let Some(e) = scan_error {
        col.push(widget::warning(e.to_owned()).into());
    }
    if devices.is_empty() {
        col.push(widget::text::title4(fl!("no-device")).into());
        col.push(widget::text::body(fl!("no-device-help")).into());
    } else {
        let names: Vec<String> = devices.iter().map(|d| format!("{} ({:04x}:{:04x})", d.name, d.vid, d.pid)).collect();
        col.push(
            widget::settings::section()
                .add(widget::settings::item(fl!("device"), widget::dropdown(names, active, Message::Select)))
                .into(),
        );
    }

    if let Some(d) = active.and_then(|i| devices.get(i)) {
        let yes_no = |b: bool| if b { fl!("yes") } else { fl!("no") };
        let c = ctx.caps;
        let info = widget::settings::section()
            .title(fl!("details"))
            .add(widget::settings::item(fl!("platform"), widget::text::body(format!("{:?}", d.platform))))
            .add(widget::settings::item(fl!("model"), widget::text::body(d.model.unwrap_or("?").to_owned())))
            .add_maybe(d.firmware.clone().map(|f| widget::settings::item(fl!("firmware"), widget::text::body(f))))
            .add(widget::settings::item(fl!("manufacturer"), widget::text::body(d.manufacturer.clone())))
            .add(widget::settings::item(fl!("connection"), widget::text::body(format!("{:?}", d.link))))
            .add(widget::settings::item(fl!("tab-colors"), widget::text::body(yes_no(c.per_key_rgb))))
            .add(widget::settings::item(fl!("tab-remap"), widget::text::body(yes_no(c.remap))))
            .add(widget::settings::item(fl!("image"), widget::text::body(yes_no(c.screen_rgb565 || c.screen_qgif))));
        col.push(info.into());
        if d.model.is_none() {
            col.push(widget::warning(fl!("unknown-model")).into());
        }
        let mut ifaces = widget::settings::section().title(fl!("interfaces"));
        for i in &d.interfaces {
            ifaces = ifaces.add(widget::text::monotext(i.clone()));
        }
        col.push(ifaces.into());
    }

    col.push(
        widget::button::standard(fl!("rescan"))
            .on_press_maybe((!ctx.busy).then_some(Message::Rescan))
            .into(),
    );
    widget::settings::view_column(col).spacing(space.space_m).into()
}
