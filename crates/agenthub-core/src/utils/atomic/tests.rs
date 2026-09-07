use super::*;

#[test]
fn creates_and_replaces_destination_without_leaking_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("config.txt");

    atomic_write(&path, b"first").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"first");

    atomic_write(&path, b"second").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"second");
    assert_eq!(
        std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
        1
    );
}

#[test]
fn restored_files_roll_back_earlier_writes_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first.txt");
    let second = dir.path().join("second.txt");
    atomic_write(&first, b"old-first").unwrap();
    atomic_write(&second, b"old-second").unwrap();

    let err = with_restored_files(&[&first, &second], || -> Result<()> {
        atomic_write(&first, b"new-first")?;
        Err(AppError::InvalidArg("boom".into()))
    })
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert_eq!(std::fs::read(&first).unwrap(), b"old-first");
    assert_eq!(std::fs::read(&second).unwrap(), b"old-second");
}

#[test]
fn restored_files_delete_newly_created_path_on_failure() {
    let dir = tempfile::tempdir().unwrap();
    let created = dir.path().join("created.txt");
    let err = with_restored_files(&[&created], || -> Result<()> {
        atomic_write(&created, b"new")?;
        Err(AppError::InvalidArg("boom".into()))
    })
    .unwrap_err();
    assert_eq!(err.code(), "invalid_arg");
    assert!(!created.exists());
}
