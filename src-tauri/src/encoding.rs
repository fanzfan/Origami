use chardetng::EncodingDetector;
use encoding_rs::Encoding;

/// Decode raw filename bytes from a legacy (non-UTF-8) archive entry.
/// `hint` is an encoding label like "gbk", "shift_jis", or "auto".
pub fn decode_name(raw: &[u8], hint: &str) -> String {
    if hint != "auto" {
        if let Some(enc) = Encoding::for_label(hint.as_bytes()) {
            let (s, _, _) = enc.decode(raw);
            return s.into_owned();
        }
    }
    if let Ok(s) = std::str::from_utf8(raw) {
        return s.to_string();
    }
    let mut det = EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(raw, true);
    // We only reach here for non-UTF-8 byte sequences, so deny UTF-8 guesses.
    let enc = det.guess(None, chardetng::Utf8Detection::Deny);
    let (s, _, _) = enc.decode(raw);
    s.into_owned()
}
