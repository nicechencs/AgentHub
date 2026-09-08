use super::{pick_file_default_title, DEFAULT_PICK_FILE_TITLE};
use crate::tray_i18n::TrayUiLanguage;

#[test]
fn pick_file_default_title_zh_en() {
    assert_eq!(
        pick_file_default_title(TrayUiLanguage::Zh),
        DEFAULT_PICK_FILE_TITLE
    );
    assert_eq!(pick_file_default_title(TrayUiLanguage::Zh), "选择文件");
    assert_eq!(pick_file_default_title(TrayUiLanguage::En), "Select file");
}
