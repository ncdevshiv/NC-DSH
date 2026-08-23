use serde_json::{Map, Value};

use crate::{
    ClassicError, ClassicErrorCode, ClassicPageLoadStrategy, ClassicUnhandledPromptBehavior,
};

pub const CLASSIC_BROWSER_NAME: &str = "moli";

pub fn matched_capabilities_from_new_session_params(
    params: &Value,
) -> Result<Map<String, Value>, ClassicError> {
    let capabilities = classic_capabilities_object(params)?;
    let always_match = capabilities
        .get("alwaysMatch")
        .map(capability_set_object)
        .transpose()?
        .unwrap_or_default();
    validate_browser_name(&always_match)?;

    let first_match = match capabilities.get("firstMatch") {
        None => vec![Map::new()],
        Some(Value::Array(entries)) if entries.is_empty() => {
            return Err(ClassicError::new(
                ClassicErrorCode::InvalidArgument,
                "firstMatch must contain at least one entry",
            ));
        }
        Some(Value::Array(entries)) => entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                capability_set_object(entry).map_err(|_| {
                    ClassicError::new(
                        ClassicErrorCode::InvalidArgument,
                        format!("firstMatch entry {index} must be an object"),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(ClassicError::new(
                ClassicErrorCode::InvalidArgument,
                "firstMatch must be an array",
            ));
        }
    };

    for candidate in &first_match {
        validate_browser_name(candidate)?;
    }

    let mut merged = Vec::with_capacity(first_match.len());
    for (index, candidate) in first_match.into_iter().enumerate() {
        if candidate
            .keys()
            .any(|capability| always_match.contains_key(capability))
        {
            return Err(ClassicError::new(
                ClassicErrorCode::InvalidArgument,
                format!("unable to merge alwaysMatch with firstMatch entry {index}"),
            ));
        }
        let mut capabilities = always_match.clone();
        capabilities.extend(candidate);
        merged.push(capabilities);
    }

    merged
        .into_iter()
        .find(browser_name_matches)
        .ok_or_else(|| {
            ClassicError::new(
                ClassicErrorCode::SessionNotCreated,
                "No matching capabilities found",
            )
        })
}

pub fn page_load_strategy_from_capabilities(
    capabilities: &Map<String, Value>,
) -> Result<ClassicPageLoadStrategy, ClassicError> {
    capabilities
        .get("pageLoadStrategy")
        .map(parse_page_load_strategy_value)
        .unwrap_or_else(|| Ok(ClassicPageLoadStrategy::default()))
}

pub fn page_load_strategy_from_new_session_params(
    params: &Value,
) -> Result<ClassicPageLoadStrategy, ClassicError> {
    let capabilities = matched_capabilities_from_new_session_params(params)?;
    page_load_strategy_from_capabilities(&capabilities)
}

pub fn unhandled_prompt_behavior_from_capabilities(
    capabilities: &Map<String, Value>,
) -> Result<ClassicUnhandledPromptBehavior, ClassicError> {
    if capabilities.contains_key("unhandledPromptBehavior") {
        return ClassicUnhandledPromptBehavior::from_capability(
            capabilities.get("unhandledPromptBehavior"),
        );
    }
    Ok(ClassicUnhandledPromptBehavior::default())
}

pub fn unhandled_prompt_behavior_from_new_session_params(
    params: &Value,
) -> Result<ClassicUnhandledPromptBehavior, ClassicError> {
    let capabilities = matched_capabilities_from_new_session_params(params)?;
    unhandled_prompt_behavior_from_capabilities(&capabilities)
}

fn classic_capabilities_object(params: &Value) -> Result<Map<String, Value>, ClassicError> {
    let Some(capabilities) = params.get("capabilities") else {
        return Ok(Map::new());
    };
    capabilities.as_object().cloned().ok_or_else(|| {
        ClassicError::new(
            ClassicErrorCode::InvalidArgument,
            "capabilities must be an object",
        )
    })
}

fn capability_set_object(value: &Value) -> Result<Map<String, Value>, ClassicError> {
    value.as_object().cloned().ok_or_else(|| {
        ClassicError::new(
            ClassicErrorCode::InvalidArgument,
            "capability set must be an object",
        )
    })
}

fn validate_browser_name(capabilities: &Map<String, Value>) -> Result<(), ClassicError> {
    match capabilities.get("browserName") {
        None | Some(Value::Null) | Some(Value::String(_)) => Ok(()),
        Some(_) => Err(ClassicError::new(
            ClassicErrorCode::InvalidArgument,
            "browserName must be a string or null",
        )),
    }
}

fn browser_name_matches(capabilities: &Map<String, Value>) -> bool {
    match capabilities.get("browserName") {
        None | Some(Value::Null) => true,
        Some(Value::String(browser_name)) => browser_name == CLASSIC_BROWSER_NAME,
        Some(_) => false,
    }
}

fn parse_page_load_strategy_value(value: &Value) -> Result<ClassicPageLoadStrategy, ClassicError> {
    match value.as_str() {
        Some("none") => Ok(ClassicPageLoadStrategy::None),
        Some("eager") => Ok(ClassicPageLoadStrategy::Eager),
        Some("normal") => Ok(ClassicPageLoadStrategy::Normal),
        Some(_) => Err(ClassicError::new(
            ClassicErrorCode::InvalidArgument,
            "pageLoadStrategy must be none, eager, or normal",
        )),
        None => Err(ClassicError::new(
            ClassicErrorCode::InvalidArgument,
            "pageLoadStrategy must be a string",
        )),
    }
}
