// NOTE: I'm trying to keep the parser simple, so I'm avoiding a lot of optimizations that would
// make either maintaining or using it more complex. The provided benchmark shows that the current
// parser churns through data over 500 MiB/s on my computer, so I don't think there is much use. If
// for whatever reason you _do_ need a ridiculously fast SRT parser, then these are my
// recommendations:
// - Enumerating the lines is used for providing feedback on what line parsing, but is costly
//   - You could just rip it all out entirely
//   - You could not keep track unless there is a failure then fallback and reparse to get more info
// - Each `text` is a `String`, so that involves a lot of allocations
//   - If you don't need mutations you could just use a `&str`
//   - If you do need mutations then you could use a `Cow` (potentially a smaller one from `beef`)
// - `parse_{two,three}_digit_ascii_num` probably isn't optimial
//   - You could do wrapping shifts by `b'0'`, pack it into a number, and _then_ bounds check
// - The timestamp line is a fixed length
//   - You could verify length and then chunk out pieces instead of using an iterator
//   - Maybe there could be some mask or something for quick validation

use std::str::Bytes;

use crate::{
    error::{Error, Result},
    time::Timestamp,
    Subtitle,
};

fn parse_three_digit_ascii_num(bytes: &mut Bytes<'_>) -> Option<u16> {
    let hundreds = bytes.next().and_then(parse_ascii_digit)?;
    let the_rest = parse_two_digit_ascii_num(bytes)?;

    Some(u16::from(hundreds) * 100 + u16::from(the_rest))
}

fn parse_two_digit_ascii_num(bytes: &mut Bytes<'_>) -> Option<u8> {
    let (tens, ones) = bytes
        .next()
        .and_then(parse_ascii_digit)
        .zip(bytes.next().and_then(parse_ascii_digit))?;

    Some(tens * 10 + ones)
}

fn parse_ascii_digit(b: u8) -> Option<u8> {
    if b.is_ascii_digit() {
        Some(b - b'0')
    } else {
        None
    }
}

// Of the form '01:23:45,678'
fn parse_ts(bytes: &mut Bytes<'_>) -> Option<Timestamp> {
    let hours = parse_two_digit_ascii_num(bytes)?;
    (bytes.next()? == b':').then_some(())?;
    let minutes = parse_two_digit_ascii_num(bytes)?;
    (bytes.next()? == b':').then_some(())?;
    let seconds = parse_two_digit_ascii_num(bytes)?;
    (bytes.next()? == b',').then_some(())?;
    let millis = parse_three_digit_ascii_num(bytes)?;

    Timestamp::new(hours, minutes, seconds, millis)
}

fn parse_ts_divider(bytes: &mut Bytes<'_>) -> Option<()> {
    (&[
        bytes.next()?,
        bytes.next()?,
        bytes.next()?,
        bytes.next()?,
        bytes.next()?,
    ] == b" --> ")
        .then_some(())
}

pub fn from_str(subtitles: &str) -> Result<Vec<Subtitle>> {
    let mut parsed: Vec<Subtitle> = Vec::new();
    let mut lines = subtitles.lines().enumerate();

    'outer: while let Some(mut pair) = lines.next() {
        'empty_line_eater: loop {
            if pair.1.is_empty() {
                pair = match lines.next() {
                    Some(pair) => pair,
                    None => {
                        break 'outer;
                    }
                };
            } else {
                break 'empty_line_eater;
            }
        }

        // Parse the id
        let (line_num, line) = pair;
        if !line.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::invalid_id(line_num));
        }

        // Parse the timestamp and duration
        let (line_num, line) = lines.next().ok_or(Error::invalid_ts_line(line_num + 1))?;
        let mut bytes = line.bytes();
        let start = parse_ts(&mut bytes).ok_or(Error::invalid_ts_start(line_num))?;
        parse_ts_divider(&mut bytes).ok_or(Error::invalid_ts_divider(line_num))?;
        let end = parse_ts(&mut bytes).ok_or(Error::invalid_ts_end(line_num))?;
        if end < start {
            return Err(Error::ts_end_before_start(line_num));
        }
        let duration = end - start;
        // Trailing bytes
        if bytes.next() != None {
            return Err(Error::invalid_ts_line(line_num));
        }

        let mut text = lines
            .next()
            .and_then(|(_, line)| {
                let trimmed = line.trim_end_matches('\r');
                (!trimmed.is_empty()).then_some(trimmed)
            })
            .ok_or(Error::missing_text(line_num + 1))?
            .to_owned();
        for (_, line) in lines.by_ref() {
            let trimmed = line.trim_end_matches('\r');
            if trimmed.is_empty() {
                break;
            }

            text.push('\n');
            text.push_str(trimmed);
        }

        parsed.push(Subtitle {
            start,
            duration,
            text,
        });
    }

    Ok(parsed)
}
