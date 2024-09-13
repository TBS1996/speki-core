use std::path::PathBuf;

pub fn get_import_csv() -> PathBuf {
    get_share_path().join("import.csv")
}

pub fn get_cards_path() -> PathBuf {
    get_share_path().join("cards")
}

pub fn get_ml_path() -> PathBuf {
    get_share_path().join("ml/")
}

pub fn get_runmodel_path() -> PathBuf {
    get_ml_path().join("runmodel.py")
}

pub fn get_media_path() -> PathBuf {
    get_share_path().join("media/")
}

#[cfg(not(test))]
pub fn get_share_path() -> PathBuf {
    let home = dirs::home_dir().unwrap();
    home.join(".local/share/speki/")
}

#[cfg(test)]
pub fn get_share_path() -> PathBuf {
    PathBuf::from("./test_dir/")
}
