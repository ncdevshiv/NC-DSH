/// A rooted Web IDL callback function and the Realm anchors captured during
/// conversion.
///
/// The callback is stored as an object because ECMAScript callable proxies are
/// valid callback functions even though they are not ordinary V8 `Function`
/// objects. Construction validates `IsCallable` once and makes a non-callable
/// state unrepresentable.
pub struct WebIdlCallbackFunction {
    callback: v8::Global<v8::Object>,
    relevant_context: v8::Global<v8::Context>,
    incumbent_context: v8::Global<v8::Context>,
}

impl WebIdlCallbackFunction {
    pub fn try_new<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        callback: v8::Local<'s, v8::Object>,
        relevant_context: v8::Local<'s, v8::Context>,
        incumbent_context: v8::Local<'s, v8::Context>,
    ) -> Option<Self> {
        callback.is_callable().then(|| Self {
            callback: v8::Global::new(scope, callback),
            relevant_context: v8::Global::new(scope, relevant_context),
            incumbent_context: v8::Global::new(scope, incumbent_context),
        })
    }

    /// Creates an invocation snapshot that no longer borrows the API owner.
    ///
    /// The owner can release its registry borrow before user code runs and
    /// synchronously reenters registration, retirement, or the owning API.
    pub fn prepare(&self, scope: &mut v8::PinScope<'_, '_>) -> PreparedWebIdlCallbackFunction {
        PreparedWebIdlCallbackFunction::try_new(
            scope,
            v8::Local::new(scope, &self.callback),
            v8::Local::new(scope, &self.relevant_context),
            v8::Local::new(scope, &self.incumbent_context),
        )
        .expect("an owned callback function must remain callable when prepared")
    }

    pub fn matches<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
        candidate: v8::Local<'s, v8::Object>,
    ) -> bool {
        v8::Local::new(scope, &self.callback).strict_equals(candidate.into())
    }

    pub fn value<'s>(&self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Value> {
        v8::Local::new(scope, &self.callback).into()
    }

    pub fn relevant_context<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> v8::Local<'s, v8::Context> {
        v8::Local::new(scope, &self.relevant_context)
    }

    pub fn incumbent_context<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> v8::Local<'s, v8::Context> {
        v8::Local::new(scope, &self.incumbent_context)
    }
}

/// An independently rooted callback-function snapshot ready for synchronous
/// invocation.
///
/// This value owns only the Web IDL callback facts. Page, Document, worker-run,
/// task, exception, and return-value policy remain with its API owner.
pub struct PreparedWebIdlCallbackFunction {
    callback: v8::Global<v8::Object>,
    relevant_context: v8::Global<v8::Context>,
    incumbent_context: v8::Global<v8::Context>,
}

impl PreparedWebIdlCallbackFunction {
    /// Roots one synchronous invocation snapshot from V8-traced owner state.
    ///
    /// Some DOM owners keep the callback and its context anchors in private
    /// V8 slots so JavaScript garbage collection can trace callback↔owner
    /// cycles. The snapshot roots those values only while one delivery is
    /// being prepared and invoked.
    pub fn try_new<'s>(
        scope: &mut v8::PinScope<'s, '_>,
        callback: v8::Local<'s, v8::Object>,
        relevant_context: v8::Local<'s, v8::Context>,
        incumbent_context: v8::Local<'s, v8::Context>,
    ) -> Option<Self> {
        callback.is_callable().then(|| Self {
            callback: v8::Global::new(scope, callback),
            relevant_context: v8::Global::new(scope, relevant_context),
            incumbent_context: v8::Global::new(scope, incumbent_context),
        })
    }

    pub fn callback<'s>(&self, scope: &mut v8::PinScope<'s, '_>) -> v8::Local<'s, v8::Object> {
        v8::Local::new(scope, &self.callback)
    }

    pub fn relevant_context<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> v8::Local<'s, v8::Context> {
        v8::Local::new(scope, &self.relevant_context)
    }

    pub fn incumbent_context<'s>(
        &self,
        scope: &mut v8::PinScope<'s, '_>,
    ) -> v8::Local<'s, v8::Context> {
        v8::Local::new(scope, &self.incumbent_context)
    }
}
