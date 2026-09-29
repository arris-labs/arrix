use super::*;
use crate::frozen::BlobRef;

fn files() -> MemorySource {
    let blob = BlobRef::of(&[7; 300], "arrisbody");
    MemorySource(BTreeMap::from([
        (DOCUMENT_JSON.into(), b"{\n  \"schema\": 2\n}\n".to_vec()),
        (PARAMS_JSON.into(), b"{}\n".repeat(50)),
        ("parts/0000000000001.json".into(), b"{}\n".to_vec()),
        ("plugins/gears/data.json".into(), b"[]".to_vec()),
        (blob.path(), vec![7; 300]),
    ]))
}

/// A one-entry zip by hand, so a test can say what an ordinary tool may.
fn raw(name: &str, method: u16, data: &[u8], size: u32, crc: u32) -> Vec<u8> {
    let mut out = Vec::new();
    put32(&mut out, LOCAL);
    for v in [20, 0, method, 0, DATE] {
        put16(&mut out, v);
    }
    put32(&mut out, crc);
    put32(&mut out, data.len() as u32);
    put32(&mut out, size);
    put16(&mut out, name.len() as u16);
    put16(&mut out, 0);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(data);
    out
}

/// Central directory and end record for `entries`: (name, method, csize,
/// size, crc, local offset).
fn finish(mut out: Vec<u8>, entries: &[(&str, u16, u32, u32, u32, u32)]) -> Vec<u8> {
    let start = out.len() as u32;
    for &(name, method, csize, size, crc, offset) in entries {
        put32(&mut out, CENTRAL);
        for v in [20, 20, 0, method, 0, DATE] {
            put16(&mut out, v);
        }
        put32(&mut out, crc);
        put32(&mut out, csize);
        put32(&mut out, size);
        put16(&mut out, name.len() as u16);
        for _ in 0..4 {
            put16(&mut out, 0);
        }
        put32(&mut out, 0);
        put32(&mut out, offset);
        out.extend_from_slice(name.as_bytes());
    }
    let size = out.len() as u32 - start;
    put32(&mut out, END);
    put16(&mut out, 0);
    put16(&mut out, 0);
    put16(&mut out, entries.len() as u16);
    put16(&mut out, entries.len() as u16);
    put32(&mut out, size);
    put32(&mut out, start);
    put16(&mut out, 0);
    out
}

fn stored(name: &str, data: &[u8]) -> Vec<u8> {
    let crc = crc32fast::hash(data);
    let n = data.len() as u32;
    finish(
        raw(name, STORED, data, n, crc),
        &[(name, STORED, n, n, crc, 0)],
    )
}

#[test]
fn the_same_files_give_the_same_bytes_and_read_back() {
    let a = to_zip(&files()).unwrap();
    assert_eq!(a, to_zip(&files()).unwrap());
    assert_eq!(from_zip(&a).unwrap().0, files().0);
}

#[test]
fn json_is_deflated_and_blobs_are_stored() {
    let bytes = to_zip(&files()).unwrap();
    let r = Reader(&bytes);
    let mut at = r.u32(bytes.len() - 22 + 16).unwrap() as usize;
    let mut seen = BTreeMap::new();
    while r.u32(at) == Ok(CENTRAL) {
        let n = usize::from(r.u16(at + 28).unwrap());
        let name = std::str::from_utf8(r.at(at + 46, n).unwrap())
            .unwrap()
            .to_owned();
        seen.insert(name, r.u16(at + 10).unwrap());
        at += 46 + n;
    }
    assert_eq!(seen.len(), 5);
    for (name, method) in seen {
        let want = if name.starts_with("blobs/") {
            STORED
        } else {
            DEFLATED
        };
        assert_eq!(method, want, "{name}");
    }
}

#[test]
fn the_timestamp_is_fixed_and_entries_are_in_path_order() {
    let bytes = to_zip(&files()).unwrap();
    assert_eq!(&bytes[10..14], &[0, 0, 0x21, 0], "1980-01-01 00:00:00");
    let names: Vec<_> = files().0.into_keys().collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    let mut at = 0;
    let mut in_zip = Vec::new();
    while Reader(&bytes).u32(at) == Ok(LOCAL) {
        let r = Reader(&bytes);
        let n = usize::from(r.u16(at + 26).unwrap());
        in_zip.push(String::from_utf8(r.at(at + 30, n).unwrap().to_vec()).unwrap());
        at += 30 + n + r.u32(at + 18).unwrap() as usize;
    }
    assert_eq!(in_zip, names);
}

#[test]
fn an_entry_outside_the_tree_is_refused() {
    for name in [
        "notes.txt",
        "../document.json",
        "/document.json",
        "parts",
        "parts/",
        "x/document.json",
        "parts//a.json",
        "a\\b",
        "params.json/x",
    ] {
        assert!(
            matches!(
                from_zip(&stored(name, b"{}")),
                Err(ZipError::OutsideTree(_))
            ),
            "{name}"
        );
    }
    assert!(from_zip(&stored("blobs/x.bin", b"{}")).is_ok());
}

#[test]
fn a_duplicate_entry_is_refused() {
    let one = raw("params.json", STORED, b"{}", 2, crc32fast::hash(b"{}"));
    let second = one.len() as u32;
    let mut both = one.clone();
    both.extend(one);
    let crc = crc32fast::hash(b"{}");
    let zip = finish(
        both,
        &[
            ("params.json", STORED, 2, 2, crc, 0),
            ("params.json", STORED, 2, 2, crc, second),
        ],
    );
    assert!(matches!(from_zip(&zip), Err(ZipError::Duplicate(n)) if n == "params.json"));
}

#[test]
fn a_bad_checksum_size_method_or_shape_is_refused() {
    let mut zip = to_zip(&files()).unwrap();
    // Flip the first data byte of the first entry, the stored blob.
    let blob = files().0.into_keys().next().unwrap();
    assert!(blob.starts_with("blobs/"));
    zip[30 + blob.len()] ^= 0xff;
    assert!(matches!(from_zip(&zip), Err(ZipError::Corrupt(_))));

    let crc = crc32fast::hash(b"{}");
    let wrong_size = finish(
        raw("params.json", STORED, b"{}", 3, crc),
        &[("params.json", STORED, 2, 3, crc, 0)],
    );
    assert!(matches!(from_zip(&wrong_size), Err(ZipError::Corrupt(_))));
    let zstd = finish(
        raw("params.json", 93, b"{}", 2, crc),
        &[("params.json", 93, 2, 2, crc, 0)],
    );
    assert!(matches!(from_zip(&zstd), Err(ZipError::Unsupported(_))));

    assert!(matches!(from_zip(b""), Err(ZipError::Malformed(_))));
    assert!(matches!(
        from_zip(b"PK\x03\x04 not really"),
        Err(ZipError::Malformed(_))
    ));
    let cut = &to_zip(&files()).unwrap()[..40];
    assert!(from_zip(cut).is_err());
}

#[test]
fn a_directory_entry_of_an_ordinary_tool_is_skipped() {
    let crc = crc32fast::hash(b"{}");
    let mut out = raw("parts/", STORED, b"", 0, 0);
    let second = out.len() as u32;
    out.extend(raw("parts/a.json", STORED, b"{}", 2, crc));
    let zip = finish(
        out,
        &[
            ("parts/", STORED, 0, 0, 0, 0),
            ("parts/a.json", STORED, 2, 2, crc, second),
        ],
    );
    let read = from_zip(&zip).unwrap();
    assert_eq!(read.0.keys().collect::<Vec<_>>(), ["parts/a.json"]);
}
