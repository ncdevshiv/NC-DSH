#[derive(Debug)]
pub(crate) enum CustomElementDefineError {
    InvalidName(String),
    NameAlreadyDefined(String),
    ConstructorAlreadyRegistered,
    InvalidExtendsTarget(String),
    DefinitionAlreadyRunning,
    ConstructorNotConstructable,
    InvalidPrototype,
    InvalidCallback(&'static str),
    InvalidSequence(&'static str),
    PendingException,
}

impl CustomElementDefineError {
    pub(crate) fn message(&self) -> String {
        match self {
            Self::InvalidName(name) => format!("Invalid custom element name `{name}`"),
            Self::NameAlreadyDefined(name) => format!("Custom element `{name}` is already defined"),
            Self::ConstructorAlreadyRegistered => {
                "Custom element constructor is already registered".to_owned()
            }
            Self::InvalidExtendsTarget(name) => {
                format!("Invalid built-in extends target `{name}`")
            }
            Self::DefinitionAlreadyRunning => {
                "Custom element definition is already running".to_owned()
            }
            Self::ConstructorNotConstructable => {
                "Custom element constructor is not constructable".to_owned()
            }
            Self::InvalidPrototype => {
                "Custom element constructor prototype must be an object".to_owned()
            }
            Self::InvalidCallback(callback_name) => {
                format!("Custom element callback `{callback_name}` must be a function")
            }
            Self::InvalidSequence(property_name) => {
                format!("Custom element `{property_name}` must be an iterable")
            }
            Self::PendingException => "Custom element definition threw an exception".to_owned(),
        }
    }

    pub(crate) fn dom_exception_name(&self) -> &'static str {
        match self {
            Self::InvalidName(_) => "SyntaxError",
            Self::NameAlreadyDefined(_)
            | Self::ConstructorAlreadyRegistered
            | Self::InvalidExtendsTarget(_)
            | Self::DefinitionAlreadyRunning => "NotSupportedError",
            Self::ConstructorNotConstructable
            | Self::InvalidPrototype
            | Self::InvalidCallback(_)
            | Self::InvalidSequence(_)
            | Self::PendingException => "TypeError",
        }
    }

    pub(crate) fn type_error_message(&self) -> Option<String> {
        match self {
            Self::ConstructorNotConstructable
            | Self::InvalidPrototype
            | Self::InvalidCallback(_)
            | Self::InvalidSequence(_) => Some(self.message()),
            Self::InvalidName(_)
            | Self::NameAlreadyDefined(_)
            | Self::ConstructorAlreadyRegistered
            | Self::InvalidExtendsTarget(_)
            | Self::DefinitionAlreadyRunning
            | Self::PendingException => None,
        }
    }

    pub(crate) fn is_pending_exception(&self) -> bool {
        matches!(self, Self::PendingException)
    }
}
