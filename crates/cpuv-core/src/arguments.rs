use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

pub fn windows_split(text: &str) -> Result<Vec<String>> {
    let mut result = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let mut count = 1;
                while chars.peek() == Some(&'\\') {
                    chars.next();
                    count += 1;
                }
                if chars.peek() == Some(&'"') {
                    chars.next();
                    word.extend(std::iter::repeat_n('\\', count / 2));
                    if count % 2 == 1 {
                        word.push('"');
                    } else {
                        quoted = !quoted;
                    }
                } else {
                    word.extend(std::iter::repeat_n('\\', count));
                }
                started = true;
            }
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    result.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    ensure!(!quoted, "Unclosed quote in compiler response file");
    if started {
        result.push(word);
    }
    Ok(result)
}

pub fn expand(input: &[OsString], cwd: &Path, msvc: bool) -> Result<Vec<OsString>> {
    fn visit(
        input: &[OsString],
        cwd: &Path,
        msvc: bool,
        seen: &mut BTreeSet<PathBuf>,
        bytes: &mut usize,
        result: &mut Vec<OsString>,
    ) -> Result<()> {
        ensure!(seen.len() <= 16, "Response file nesting limit exceeded");
        for argument in input {
            if let Some(name) = argument.to_str().and_then(|s| s.strip_prefix('@')) {
                let path = cwd
                    .join(name)
                    .canonicalize()
                    .with_context(|| format!("Reading response file {name}"))?;
                ensure!(
                    seen.insert(path.clone()),
                    "Cyclic response file {}",
                    path.display()
                );
                let length = fs::metadata(&path)?.len();
                ensure!(
                    length <= 8 * 1024 * 1024 && *bytes + length as usize <= 8 * 1024 * 1024,
                    "Response files exceed 8 MiB"
                );
                let data = fs::read(&path)?;
                *bytes += data.len();
                let text = if data.starts_with(&[0xff, 0xfe]) {
                    ensure!((data.len() - 2) % 2 == 0, "Truncated UTF-16 response file");
                    String::from_utf16(
                        &data[2..]
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|b| u16::from_le_bytes([b[0], b[1]]))
                            .collect::<Vec<_>>(),
                    )?
                } else {
                    String::from_utf8(data)?
                        .trim_start_matches('\u{feff}')
                        .to_owned()
                };
                let parsed = if msvc {
                    windows_split(&text)?
                } else {
                    shlex::split(&text.replace("\r\n", "\n"))
                        .context("Invalid GNU response file quoting")?
                };
                visit(
                    &parsed.into_iter().map(OsString::from).collect::<Vec<_>>(),
                    cwd,
                    msvc,
                    seen,
                    bytes,
                    result,
                )?;
                seen.remove(&path);
            } else {
                result.push(argument.clone());
            }
            ensure!(
                result.len() <= 200_000,
                "Expanded compiler argument count exceeds limit"
            );
        }
        Ok(())
    }
    let mut result = Vec::new();
    visit(input, cwd, msvc, &mut BTreeSet::new(), &mut 0, &mut result)?;
    Ok(result)
}

pub fn quote(value: &str, msvc: bool) -> String {
    if !msvc {
        return format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""));
    }
    let mut output = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            '\\',
            if c == '"' { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        output.push(c);
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}

pub fn shorten(
    args: Vec<OsString>,
    msvc: bool,
) -> Result<(Vec<OsString>, Option<tempfile::NamedTempFile>)> {
    if args
        .iter()
        .map(|a| a.to_string_lossy().len() + 3)
        .sum::<usize>()
        < 24_000
    {
        return Ok((args, None));
    }
    use std::io::Write;
    let mut file = tempfile::Builder::new()
        .prefix("cpuv-arguments-")
        .suffix(".rsp")
        .tempfile()?;
    let text = args
        .iter()
        .map(|a| {
            a.to_str()
                .map(|s| quote(s, msvc))
                .context("Compiler arguments must be Unicode for response files")
        })
        .collect::<Result<Vec<_>>>()?
        .join("\n");
    if msvc {
        file.write_all(&[0xff, 0xfe])?;
        for unit in text.encode_utf16() {
            file.write_all(&unit.to_le_bytes())?;
        }
    } else {
        file.write_all(text.as_bytes())?;
    }
    file.flush()?;
    Ok((
        vec![OsString::from(format!("@{}", file.path().display()))],
        Some(file),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_quoting_round_trips() {
        for value in ["", "a b", r"C:\Program Files\", "a\"b", "x\\\"y", "normal"] {
            assert_eq!(windows_split(&quote(value, true)).unwrap(), [value]);
        }
    }
    #[test]
    fn cyclic_responses_fail_without_expanding_forever() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.rsp"), "@a.rsp").unwrap();
        assert!(expand(&["@a.rsp".into()], dir.path(), false).is_err());
    }
    #[test]
    fn long_commands_round_trip_through_both_response_formats() {
        let args: Vec<OsString> = (0..1000)
            .map(|i| format!("-DLABEL_{i}=quoted value \"with spaces\"").into())
            .collect();
        for msvc in [false, true] {
            let (short, file) = shorten(args.clone(), msvc).unwrap();
            assert!(file.is_some());
            assert_eq!(expand(&short, Path::new("."), msvc).unwrap(), args);
        }
    }
    #[test]
    fn gnu_response_file_accepts_windows_line_endings() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("input.rsp"),
            "\"main file.cpp\"\r\n-DOK\r\n",
        )
        .unwrap();
        assert_eq!(
            expand(&["@input.rsp".into()], dir.path(), false).unwrap(),
            vec![OsString::from("main file.cpp"), OsString::from("-DOK")]
        );
    }
}
