use super::super::*;

pub(super) fn install_structural_template_accessors<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    _template: v8::Local<'s, v8::ObjectTemplate>,
    installer: SpecializedTemplateInstaller,
) -> bool {
    matches!(
        installer,
        SpecializedTemplateInstaller::ShadowRoot
            | SpecializedTemplateInstaller::HtmlTemplateElement
            | SpecializedTemplateInstaller::HtmlLiElement
            | SpecializedTemplateInstaller::HtmlOListElement
            | SpecializedTemplateInstaller::HtmlOptGroupElement
            | SpecializedTemplateInstaller::HtmlQuoteElement
            | SpecializedTemplateInstaller::HtmlTableElement
            | SpecializedTemplateInstaller::HtmlTableSectionElement
            | SpecializedTemplateInstaller::HtmlTableRowElement
            | SpecializedTemplateInstaller::HtmlTableCellElement
            | SpecializedTemplateInstaller::HtmlBodyElement
            | SpecializedTemplateInstaller::HtmlDetailsElement
            | SpecializedTemplateInstaller::HtmlDialogElement
            | SpecializedTemplateInstaller::HtmlTimeElement
            | SpecializedTemplateInstaller::HtmlTitleElement
            | SpecializedTemplateInstaller::HtmlMetaElement
    )
}
