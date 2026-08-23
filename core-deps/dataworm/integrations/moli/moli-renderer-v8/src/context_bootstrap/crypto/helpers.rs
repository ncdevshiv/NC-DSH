use super::*;

pub(super) fn is_crypto_integer_typed_array(value: v8::Local<'_, v8::Value>) -> bool {
    value.is_int8_array()
        || value.is_uint8_array()
        || value.is_uint8_clamped_array()
        || value.is_int16_array()
        || value.is_uint16_array()
        || value.is_int32_array()
        || value.is_uint32_array()
        || value.is_big_int64_array()
        || value.is_big_uint64_array()
}

pub(super) fn crypto_algorithm_name(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Option<String> {
    // Chromium's WebCrypto lookup is ASCII-case-insensitive but length-exact:
    // names with leading/trailing whitespace do not match registered
    // algorithms. Preserve the caller-provided characters and only fold case.
    if let Ok(object) = v8::Local::<v8::Object>::try_from(value) {
        let name = object.get(scope, v8str(scope, "name").into())?;
        if name.is_undefined() {
            return None;
        }
        return js_value_to_rust_string_lossy(scope, name).map(|value| value.to_ascii_lowercase());
    }
    js_value_to_rust_string_lossy(scope, value).map(|value| value.to_ascii_lowercase())
}

fn js_value_to_rust_string_lossy(
    scope: &mut v8::PinScope<'_, '_>,
    value: v8::Local<'_, v8::Value>,
) -> Option<String> {
    if let Ok(string) = v8::Local::<v8::String>::try_from(value) {
        return Some(string.to_rust_string_lossy(scope));
    }
    value
        .to_string(scope)
        .map(|value| value.to_rust_string_lossy(scope))
}

pub(super) fn set_rejected_promise(
    scope: &mut v8::PinScope<'_, '_>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
    name: &str,
) {
    let Some(resolver) = v8::PromiseResolver::new(scope) else {
        rv.set(v8::undefined(scope).into());
        return;
    };
    let promise = resolver.get_promise(scope);
    let exception = crypto_error_value(scope, name).unwrap_or_else(|| v8::undefined(scope).into());
    let _ = resolver.reject(scope, exception);
    rv.set(promise.into());
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WebCryptoRejection {
    Type,
    NotSupported,
    InvalidAccess,
    Syntax,
    Data,
    Operation,
}

impl WebCryptoRejection {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Type => "TypeError",
            Self::NotSupported => "NotSupportedError",
            Self::InvalidAccess => "InvalidAccessError",
            Self::Syntax => "SyntaxError",
            Self::Data => "DataError",
            Self::Operation => "OperationError",
        }
    }
}

pub(super) fn set_rejected_webcrypto_promise(
    scope: &mut v8::PinScope<'_, '_>,
    rv: &mut v8::ReturnValue<'_, v8::Value>,
    rejection: WebCryptoRejection,
) {
    set_rejected_promise(scope, rv, rejection.name());
}

impl From<moli_webcrypto::WebCryptoError> for WebCryptoRejection {
    fn from(error: moli_webcrypto::WebCryptoError) -> Self {
        match error {
            moli_webcrypto::WebCryptoError::Data => Self::Data,
            moli_webcrypto::WebCryptoError::Operation => Self::Operation,
            moli_webcrypto::WebCryptoError::Syntax => Self::Syntax,
        }
    }
}

pub(super) struct PendingCryptoPromise<'s> {
    resolver: v8::Local<'s, v8::PromiseResolver>,
}

impl<'s> PendingCryptoPromise<'s> {
    pub(super) fn new(
        scope: &mut v8::PinScope<'s, '_>,
        rv: &mut v8::ReturnValue<'_, v8::Value>,
    ) -> Option<Self> {
        let resolver = v8::PromiseResolver::new(scope)?;
        let promise = resolver.get_promise(scope);
        rv.set(promise.into());
        Some(Self { resolver })
    }

    pub(super) fn resolve(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        value: v8::Local<'s, v8::Value>,
    ) {
        let _ = self.resolver.resolve(scope, value);
    }

    pub(super) fn resolver(&self) -> v8::Local<'s, v8::PromiseResolver> {
        self.resolver
    }

    pub(super) fn reject(&self, scope: &mut v8::PinScope<'s, '_>, name: &str) {
        let exception =
            crypto_error_value(scope, name).unwrap_or_else(|| v8::undefined(scope).into());
        let _ = self.resolver.reject(scope, exception);
    }

    pub(super) fn reject_webcrypto(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        rejection: WebCryptoRejection,
    ) {
        self.reject(scope, rejection.name());
    }
}

fn crypto_error_value<'s>(
    scope: &mut v8::PinScope<'s, '_>,
    name: &str,
) -> Option<v8::Local<'s, v8::Value>> {
    if name == "TypeError" {
        let name_value = v8_string(scope, name)?;
        return Some(v8::Exception::type_error(scope, name_value));
    }

    Some(crate::context_bootstrap::new_dom_exception_value(
        scope, name, name,
    ))
}
