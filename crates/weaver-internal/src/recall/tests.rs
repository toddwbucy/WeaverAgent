use super::*;

#[test]
fn only_the_empty_subject_refuses() {
    assert!(matches!(query(""), Err(EmptySubject)));
    for subject in ["a", "\0", ".", "\u{200b}"] {
        assert_eq!(query(subject).unwrap().subject(), subject);
    }
}

#[test]
fn nonempty_whitespace_is_preserved_whole() {
    for subject in [" ", "\t\r\n", "\u{a0}", "\u{2003}"] {
        let result = query(subject).unwrap();
        assert_eq!(result.subject().as_bytes(), subject.as_bytes());
        assert_eq!(result.subject().as_ptr(), subject.as_ptr());
    }
}

#[test]
fn mixed_case_and_multibyte_subjects_keep_the_original_bytes_and_pointer() {
    for subject in [
        "  ReadMe.TXT \t",
        "MiXeD CaSe",
        "caf\u{e9}",
        "cafe\u{301}",
        "\u{6771}\u{4eac} \u{1f4be}",
        "item\0name",
    ] {
        let owned = String::from(subject);
        let result = query(&owned).unwrap();
        assert_eq!(result.subject().as_bytes(), owned.as_bytes());
        assert_eq!(result.subject().as_ptr(), owned.as_ptr());
    }
}

#[test]
fn a_borrowed_subslice_keeps_its_exact_start_and_length() {
    let owned = String::from("before|caf\u{e9}|after");
    let subject = &owned[7..12];
    let result = query(subject).unwrap();
    assert_eq!(result.subject(), "caf\u{e9}");
    assert_eq!(result.subject().len(), subject.len());
    assert_eq!(result.subject().as_ptr(), subject.as_ptr());
}

#[test]
fn the_accessor_borrow_can_outlive_the_query_handle() {
    let subject = String::from("item");
    let retained = {
        let result = query(&subject).unwrap();
        result.subject()
    };
    assert_eq!(retained.as_ptr(), subject.as_ptr());
    assert_eq!(retained, subject);
}
