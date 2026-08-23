use super::super::super::*;
use crate::native_bridge::element::forms::{
    select_indexed_definer, select_indexed_deleter, select_indexed_descriptor,
    select_indexed_enumerator, select_indexed_getter, select_indexed_query, select_indexed_setter,
};

pub(super) fn install_select_option_template_accessors<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
    installer: SpecializedTemplateInstaller,
) -> bool {
    match installer {
        SpecializedTemplateInstaller::HtmlOptionElement => true,
        SpecializedTemplateInstaller::HtmlSelectElement => {
            install_select_accessors(_scope, template);
            true
        }
        _ => false,
    }
}

fn install_select_accessors<'s, 'i>(
    _scope: &mut v8::PinScope<'s, 'i, ()>,
    template: v8::Local<'s, v8::ObjectTemplate>,
) {
    template.set_indexed_property_handler(
        v8::IndexedPropertyHandlerConfiguration::new()
            .getter(select_indexed_getter)
            .setter(select_indexed_setter)
            .query(select_indexed_query)
            .deleter(select_indexed_deleter)
            .enumerator(select_indexed_enumerator)
            .definer(select_indexed_definer)
            .descriptor(select_indexed_descriptor),
    );
}
