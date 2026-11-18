//! Text forms the SDK does not export: vanilla's SNBT rendering and a custom
//! name out of a text component.
//!
//! `/cu e entityData` prints an entity's saved compound the way vanilla's
//! `/data get entity` does, and `/cu e find` matches against the entity's
//! **custom** name only. The 0.1.0 glue used the game's own `to_snbt` and a
//! custom-name read; the SDK exports neither (`EntityRef::nbt` hands over a
//! `Compound` and `EntityRef::name` answers the *display* name, which falls back
//! to the type's name), so this module carries both, with the rules vanilla's
//! `StringTagVisitor` / `StringTag.quoteAndEscape` use.
use mod_api::{Compound, Tag};

/// An entity's custom name as plain text, out of its saved compound's
/// `CustomName`.
///
/// 26.3 saves a text component (`{text:"Flossy"}` or `{literal:…}`); an older
/// or simplified name is a bare string. A component with neither key renders as
/// its SNBT, which is still readable in chat.
pub fn custom_name(nbt: &Compound) -> Option<String> {
    let tag = compound_get(nbt, "CustomName")?;
    match tag {
        Tag::String(name) => Some(name.to_string_lossy()),
        Tag::Compound(_) => {
            for key in ["text", "literal"] {
                if let Some(Tag::String(text)) = tag.get(key) {
                    return Some(text.to_string_lossy());
                }
            }
            Some(to_snbt(tag))
        }
        _ => Some(to_snbt(tag)),
    }
}

/// A tag as compact SNBT (vanilla's `Tag.toString`).
pub fn to_snbt(tag: &Tag) -> String {
    let mut out = String::new();
    visit(tag, &mut out);
    out
}

/// `compound[key]`, comparing the key as UTF-16 units (the map's own order).
fn compound_get<'a>(map: &'a Compound, key: &str) -> Option<&'a Tag> {
    let wanted: Vec<u16> = key.encode_utf16().collect();
    map.iter()
        .find(|(name, _)| name.0 == wanted)
        .map(|(_, value)| value)
}

fn visit(tag: &Tag, out: &mut String) {
    match tag {
        Tag::End => out.push_str("END"),
        Tag::Byte(value) => out.push_str(&format!("{value}b")),
        Tag::Short(value) => out.push_str(&format!("{value}s")),
        Tag::Int(value) => out.push_str(&value.to_string()),
        Tag::Long(value) => out.push_str(&format!("{value}L")),
        Tag::Float(bits) => {
            out.push_str(&java_float(f32::from_bits(*bits)));
            out.push('f');
        }
        Tag::Double(bits) => {
            out.push_str(&java_double(f64::from_bits(*bits)));
            out.push('d');
        }
        Tag::String(text) => out.push_str(&quote_and_escape(&text.to_string_lossy())),
        Tag::ByteArray(values) => {
            out.push_str("[B;");
            let parts: Vec<String> = values.iter().map(|v| format!("{}B", *v as i8)).collect();
            out.push_str(&parts.join(","));
            out.push(']');
        }
        Tag::IntArray(values) => {
            out.push_str("[I;");
            let parts: Vec<String> = values.iter().map(i32::to_string).collect();
            out.push_str(&parts.join(","));
            out.push(']');
        }
        Tag::LongArray(values) => {
            out.push_str("[L;");
            let parts: Vec<String> = values.iter().map(|v| format!("{v}L")).collect();
            out.push_str(&parts.join(","));
            out.push(']');
        }
        Tag::List(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                visit(value, out);
            }
            out.push(']');
        }
        Tag::Compound(map) => {
            out.push('{');
            // The keys are already in vanilla's UTF-16 order (the map's own
            // order); sorting converted Rust strings would move supplementary
            // characters.
            for (index, (key, value)) in map.iter().enumerate() {
                let key = key.to_string_lossy();
                if index > 0 {
                    out.push(',');
                }
                if unquoted_key(&key) {
                    out.push_str(&key);
                } else {
                    out.push_str(&quote_and_escape(&key));
                }
                out.push(':');
                visit(value, out);
            }
            out.push('}');
        }
    }
}

/// `Double.toString`: always a decimal point, `E` notation outside
/// `[1e-3, 1e7)`, and Java's names for the special values.
fn java_double(value: f64) -> String {
    java_number_text(value, &format!("{value}"), &format!("{value:e}"))
}

/// `Float.toString`, on the float's own shortest form.
fn java_float(value: f32) -> String {
    java_number_text(f64::from(value), &format!("{value}"), &format!("{value:e}"))
}

fn java_number_text(value: f64, plain: &str, scientific: &str) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    if (1.0e-3..1.0e7).contains(&value.abs()) {
        return if plain.contains('.') {
            plain.to_owned()
        } else {
            format!("{plain}.0")
        };
    }
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((scientific, "0"));
    if mantissa.contains('.') {
        format!("{mantissa}E{exponent}")
    } else {
        // Java picks the shortest form that round-trips with two decimal
        // digits too; Rust's one-decimal form matches it for the values a
        // player sees.
        format!("{value:.1e}").replace('e', "E")
    }
}

/// `SnbtGrammar.escapeControlCharacters`.
fn escape_control(character: char) -> Option<&'static str> {
    Some(match character {
        '\u{8}' => "b",
        '\u{c}' => "f",
        '\n' => "n",
        '\r' => "r",
        '\t' => "t",
        _ => return None,
    })
}

/// `StringTag.quoteAndEscape`: quote with `"` unless the text holds one, in
/// which case `'`, and escape the quote that is used.
fn quote_and_escape(input: &str) -> String {
    let mut quote = '\0';
    let mut body = String::with_capacity(input.len() + 2);
    for character in input.chars() {
        if character == '\\' {
            body.push_str("\\\\");
        } else if character != '"' && character != '\'' {
            match escape_control(character) {
                Some(escaped) => {
                    body.push('\\');
                    body.push_str(escaped);
                }
                None => body.push(character),
            }
        } else {
            if quote == '\0' {
                quote = if character == '"' { '\'' } else { '"' };
            }
            if quote == character {
                body.push('\\');
            }
            body.push(character);
        }
    }
    if quote == '\0' {
        quote = '"';
    }
    format!("{quote}{body}{quote}")
}

/// `[A-Za-z._]+[A-Za-z0-9._+-]*`, and never a bare `true`/`false`.
fn unquoted_key(key: &str) -> bool {
    if key.eq_ignore_ascii_case("true") || key.eq_ignore_ascii_case("false") {
        return false;
    }
    let mut characters = key.chars();
    let first = characters
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '.' || c == '_');
    first && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_carry_vanilla_suffixes() {
        assert_eq!(to_snbt(&Tag::Byte(-3)), "-3b");
        assert_eq!(to_snbt(&Tag::Short(7)), "7s");
        assert_eq!(to_snbt(&Tag::Int(42)), "42");
        assert_eq!(to_snbt(&Tag::Long(9)), "9L");
        assert_eq!(to_snbt(&Tag::Float(1.5f32.to_bits())), "1.5f");
        assert_eq!(to_snbt(&Tag::Double(2.0f64.to_bits())), "2.0d");
    }

    #[test]
    fn java_number_text_for_the_awkward_values() {
        // Java's Double.toString: a decimal point always, `E` notation outside
        // [1e-3, 1e7).
        assert_eq!(to_snbt(&Tag::Double(1.0f64.to_bits())), "1.0d");
        assert_eq!(to_snbt(&Tag::Double(1.0e7f64.to_bits())), "1.0E7d");
        assert_eq!(to_snbt(&Tag::Double(1.0e-4f64.to_bits())), "1.0E-4d");
        assert_eq!(to_snbt(&Tag::Double(f64::NAN.to_bits())), "NaNd");
        assert_eq!(
            to_snbt(&Tag::Double(f64::NEG_INFINITY.to_bits())),
            "-Infinityd"
        );
        assert_eq!(to_snbt(&Tag::Double((-0.0f64).to_bits())), "-0.0d");
    }

    #[test]
    fn strings_and_keys_are_quoted_as_vanilla_quotes_them() {
        // A plain string is double-quoted; a quote inside it picks the other
        // quote character, as `StringTag.quoteAndEscape` does.
        assert_eq!(
            to_snbt(&Tag::compound([("n", Tag::String("hi".into()))])),
            "{n:\"hi\"}"
        );
        assert_eq!(
            to_snbt(&Tag::compound([("n", Tag::String("say \"hi\"".into()))])),
            "{n:'say \"hi\"'}"
        );
        assert_eq!(
            to_snbt(&Tag::compound([("a b", Tag::Int(1))])),
            "{\"a b\":1}"
        );
        assert_eq!(
            to_snbt(&Tag::compound([("true", Tag::Int(1))])),
            "{\"true\":1}"
        );
        assert_eq!(to_snbt(&Tag::String("tab\there".into())), "\"tab\\there\"");
    }

    #[test]
    fn lists_and_arrays_use_vanilla_brackets() {
        assert_eq!(to_snbt(&Tag::List(vec![Tag::Int(1), Tag::Int(2)])), "[1,2]");
        assert_eq!(to_snbt(&Tag::ByteArray(vec![1, 255])), "[B;1B,-1B]");
        assert_eq!(to_snbt(&Tag::IntArray(vec![1, -2])), "[I;1,-2]");
        assert_eq!(to_snbt(&Tag::LongArray(vec![1])), "[L;1L]");
        assert_eq!(to_snbt(&Tag::End), "END");
    }

    #[test]
    fn a_compound_is_written_key_by_key_in_order() {
        let nbt = Tag::compound([
            ("Health", Tag::Float(20.0f32.to_bits())),
            (
                "CustomName",
                Tag::compound([("text", Tag::String("Flossy".into()))]),
            ),
        ]);
        assert_eq!(to_snbt(&nbt), "{CustomName:{text:\"Flossy\"},Health:20.0f}");
    }

    #[test]
    fn the_custom_name_comes_out_of_either_shape() {
        // 26.3 saves a text component; an older or simplified name is a string.
        let mut nbt: Compound = Compound::new();
        nbt.insert(
            "CustomName".into(),
            Tag::compound([("text", Tag::String("Flossy".into()))]),
        );
        assert_eq!(custom_name(&nbt), Some("Flossy".to_owned()));

        let mut plain: Compound = Compound::new();
        plain.insert("CustomName".into(), Tag::String("Mossy".into()));
        assert_eq!(custom_name(&plain), Some("Mossy".to_owned()));

        let mut literal: Compound = Compound::new();
        literal.insert(
            "CustomName".into(),
            Tag::compound([("literal", Tag::String("Pebble".into()))]),
        );
        assert_eq!(custom_name(&literal), Some("Pebble".to_owned()));

        assert_eq!(custom_name(&Compound::new()), None);
    }

    #[test]
    fn a_component_without_text_falls_back_to_its_snbt() {
        let mut nbt: Compound = Compound::new();
        nbt.insert(
            "CustomName".into(),
            Tag::compound([("translate", Tag::String("entity.example".into()))]),
        );
        assert_eq!(
            custom_name(&nbt),
            Some("{translate:\"entity.example\"}".to_owned())
        );
    }

    #[test]
    fn a_lookup_key_is_compared_as_utf16_units() {
        let mut nbt: Compound = Compound::new();
        nbt.insert("CustomName".into(), Tag::String("Mossy".into()));
        assert_eq!(custom_name(&nbt), Some("Mossy".to_owned()));
        assert_eq!(compound_get(&nbt, "Customnamed"), None);
    }
}
