use unreal_asset::types::fname::FName;

pub(crate) fn render_fname(name: &FName) -> String {
    let content = name.get_content();
    match name.get_number() {
        number if number > 0 => format!("{content}_{}", number - 1),
        _ => content,
    }
}
