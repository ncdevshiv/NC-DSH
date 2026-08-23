use super::super::*;

pub(super) fn install_media_template_accessors<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
    installer: SpecializedTemplateInstaller,
) -> bool {
    let _ = (scope, template);
    matches!(
        installer,
        SpecializedTemplateInstaller::HtmlMediaElement
            | SpecializedTemplateInstaller::HtmlAudioElement
            | SpecializedTemplateInstaller::HtmlVideoElement
    )
}
