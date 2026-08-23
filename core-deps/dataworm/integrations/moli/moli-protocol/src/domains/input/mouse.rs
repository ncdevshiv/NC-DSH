use super::*;
use chromiumoxide_cdp::cdp::browser_protocol::input::{
    DispatchMouseEventParams, DispatchMouseEventType,
};

pub(super) fn validate_dispatch_mouse_event(cmd: &Cmd<'_>) -> Result<(), &'static str> {
    if !required_enum_param_matches(
        cmd,
        "type",
        &["mousePressed", "mouseReleased", "mouseMoved", "mouseWheel"],
    ) || !optional_enum_param_matches(
        cmd,
        "button",
        &["none", "left", "middle", "right", "back", "forward"],
    ) || !optional_enum_param_matches(cmd, "pointerType", &["mouse", "pen"])
    {
        return Err("InvalidParams");
    }
    let params: DispatchMouseEventParams = match cmd.get_params() {
        Ok(Some(params)) => params,
        _ => return Err("InvalidParams"),
    };
    for value in [
        params.buttons,
        params.click_count,
        params.modifiers,
        params.twist,
    ]
    .into_iter()
    .flatten()
    {
        i32::try_from(value).map_err(|_| "InvalidParams")?;
    }
    if params.r#type == DispatchMouseEventType::MouseWheel {
        if params.delta_x.is_none() || params.delta_y.is_none() {
            return Err("InvalidParams");
        }
        return Ok(());
    }
    validate_pointer_properties(
        params.force.unwrap_or(0.0),
        params.tangential_pressure.unwrap_or(0.0),
        params.tilt_x.unwrap_or(0.0),
        params.tilt_y.unwrap_or(0.0),
        params.twist.unwrap_or(0),
    )
}

pub(super) fn validate_pointer_properties(
    force: f64,
    tangential_pressure: f64,
    tilt_x: f64,
    tilt_y: f64,
    twist: i64,
) -> Result<(), &'static str> {
    if !(0.0..=1.0).contains(&force)
        || !(-1.0..=1.0).contains(&tangential_pressure)
        || !(-90.0..=90.0).contains(&tilt_x)
        || !(-90.0..=90.0).contains(&tilt_y)
        || !(0..=359).contains(&twist)
    {
        return Err("InvalidParams");
    }
    Ok(())
}
