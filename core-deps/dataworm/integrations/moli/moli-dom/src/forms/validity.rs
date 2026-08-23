#[derive(Clone, Copy, Debug, Default)]
pub struct FormControlValidity {
    pub value_missing: bool,
    pub type_mismatch: bool,
    pub pattern_mismatch: bool,
    pub too_long: bool,
    pub too_short: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
    pub step_mismatch: bool,
    pub bad_input: bool,
    pub custom_error: bool,
}

impl FormControlValidity {
    pub fn valid(self) -> bool {
        !self.value_missing
            && !self.type_mismatch
            && !self.pattern_mismatch
            && !self.too_long
            && !self.too_short
            && !self.range_underflow
            && !self.range_overflow
            && !self.step_mismatch
            && !self.bad_input
            && !self.custom_error
    }

    pub fn validation_message(self, custom_message: &str) -> String {
        if !custom_message.is_empty() {
            return custom_message.to_owned();
        }
        if self.value_missing {
            return "Please fill out this field.".to_owned();
        }
        if self.type_mismatch {
            return "Please enter a valid value.".to_owned();
        }
        if self.pattern_mismatch {
            return "Please match the requested format.".to_owned();
        }
        if self.too_long {
            return "Please shorten this text.".to_owned();
        }
        if self.too_short {
            return "Please lengthen this text.".to_owned();
        }
        if self.range_underflow || self.range_overflow {
            return "Please select a value in the allowed range.".to_owned();
        }
        if self.step_mismatch {
            return "Please enter a valid value.".to_owned();
        }
        if self.bad_input {
            return "Please enter a number.".to_owned();
        }
        String::new()
    }
}
