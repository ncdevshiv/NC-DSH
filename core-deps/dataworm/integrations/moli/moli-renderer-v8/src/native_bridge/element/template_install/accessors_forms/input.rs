use super::super::super::*;

pub(super) fn install_input_template_accessors<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    installer: SpecializedTemplateInstaller,
) -> bool {
    matches!(installer, SpecializedTemplateInstaller::HtmlInputElement)
}
