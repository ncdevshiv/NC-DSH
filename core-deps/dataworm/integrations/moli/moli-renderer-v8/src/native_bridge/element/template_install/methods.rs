use super::super::*;

pub(super) fn install_specialized_template_methods<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    installer: SpecializedTemplateInstaller,
) {
    match installer {
        SpecializedTemplateInstaller::HtmlFormElement => {}
        SpecializedTemplateInstaller::HtmlMediaElement
        | SpecializedTemplateInstaller::HtmlAudioElement
        | SpecializedTemplateInstaller::HtmlVideoElement => {}
        SpecializedTemplateInstaller::HtmlImageElement => {}
        SpecializedTemplateInstaller::HtmlInputElement
        | SpecializedTemplateInstaller::HtmlTextAreaElement => {}
        SpecializedTemplateInstaller::HtmlSelectElement
        | SpecializedTemplateInstaller::HtmlButtonElement
        | SpecializedTemplateInstaller::HtmlFieldSetElement
        | SpecializedTemplateInstaller::HtmlOutputElement => {}
        SpecializedTemplateInstaller::HtmlCanvasElement => {}
        SpecializedTemplateInstaller::HtmlTableElement => {}
        SpecializedTemplateInstaller::HtmlTableSectionElement => {}
        SpecializedTemplateInstaller::HtmlTableRowElement => {}
        _ => {}
    }
}
