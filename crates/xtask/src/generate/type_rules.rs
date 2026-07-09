use super::model::ApiItem;

pub fn has_raw_pointer(item: &ApiItem) -> bool {
    item.arguments
        .iter()
        .any(|argument| argument.ty.contains('*') || argument.canonical_type.contains('*'))
        || item.result_type.contains('*')
        || item.result_canonical_type.contains('*')
}

pub fn has_callback_signature(item: &ApiItem) -> bool {
    let text = type_text(item);
    text.contains("(*)") || text.contains("(*") || text.contains("Func")
}

pub fn has_opaque_pointer(item: &ApiItem) -> bool {
    let text = type_text(item).replace(' ', "");
    text.contains("void*")
}

pub fn type_text(item: &ApiItem) -> String {
    let mut text = String::new();
    text.push_str(&item.result_type);
    text.push(' ');
    text.push_str(&item.result_canonical_type);
    for argument in &item.arguments {
        text.push(' ');
        text.push_str(&argument.ty);
        text.push(' ');
        text.push_str(&argument.canonical_type);
    }
    text
}
