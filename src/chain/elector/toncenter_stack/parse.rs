use anyhow::{Context, Result, bail};
use serde_json::Value;
use tycho_types::boc::Boc;
use tycho_types::cell::Cell;

pub(super) fn stack_entry_tuple(entry: &Value) -> Option<&[Value]> {
    if let Some(items) = entry.as_array()
        && items.first()?.as_str()? == "tuple"
    {
        return items.get(1)?.as_array().map(Vec::as_slice);
    }

    let object = entry.as_object()?;
    object
        .get("tuple")
        .and_then(|tuple| {
            tuple
                .get("elements")
                .and_then(Value::as_array)
                .or_else(|| tuple.as_array())
        })
        .map(Vec::as_slice)
}

pub(super) fn stack_entry_list(entry: &Value) -> Option<&[Value]> {
    if let Some(items) = entry.as_array()
        && items.first()?.as_str()? == "list"
    {
        return items
            .get(1)?
            .get("elements")
            .and_then(Value::as_array)
            .map(Vec::as_slice);
    }

    let object = entry.as_object()?;
    object
        .get("list")
        .and_then(|list| {
            list.get("elements")
                .and_then(Value::as_array)
                .or_else(|| list.as_array())
        })
        .or_else(|| object.get("elements").and_then(Value::as_array))
        .map(Vec::as_slice)
}

fn stack_entry_number_text(entry: &Value) -> Option<&str> {
    if let Some(items) = entry.as_array()
        && items.first()?.as_str()? == "num"
    {
        return items.get(1)?.as_str();
    }

    let object = entry.as_object()?;
    object.get("num").and_then(Value::as_str).or_else(|| {
        object
            .get("number")
            .and_then(|number| number.get("number"))
            .and_then(Value::as_str)
    })
}

pub(super) fn parse_stack_cell(entry: &Value) -> Result<Cell> {
    let bytes = if let Some(items) = entry.as_array() {
        if items.first().and_then(Value::as_str) == Some("cell") {
            items
                .get(1)
                .and_then(|cell| cell.get("bytes"))
                .and_then(Value::as_str)
        } else {
            None
        }
    } else {
        entry
            .get("cell")
            .and_then(|cell| cell.get("bytes"))
            .and_then(Value::as_str)
    }
    .context("stack entry is not a cell")?;

    Boc::decode_base64(bytes).context("failed to decode TON Center stack cell")
}

pub(super) fn parse_stack_hash(entry: &Value) -> Result<[u8; 32]> {
    let text = stack_entry_number_text(entry).context("stack entry is not a number")?;
    parse_u256_text(text)
}

pub(super) fn parse_stack_u32(entry: &Value) -> Result<u32> {
    let value = parse_stack_u128(entry)?;
    u32::try_from(value).context("number does not fit into u32")
}

pub(super) fn parse_stack_u128(entry: &Value) -> Result<u128> {
    let text = stack_entry_number_text(entry).context("stack entry is not a number")?;
    if let Some(hex) = text.strip_prefix("0x") {
        u128::from_str_radix(hex, 16).context("invalid hex number")
    } else {
        text.parse::<u128>().context("invalid decimal number")
    }
}

fn parse_u256_text(text: &str) -> Result<[u8; 32]> {
    if let Some(hex) = text.strip_prefix("0x") {
        return parse_u256_hex(hex);
    }

    let mut bytes = [0_u8; 32];
    for digit in text.bytes() {
        if !digit.is_ascii_digit() {
            bail!("invalid decimal u256");
        }

        let mut carry = (digit - b'0') as u16;
        for byte in bytes.iter_mut().rev() {
            let value = (*byte as u16) * 10 + carry;
            *byte = value as u8;
            carry = value >> 8;
        }
        if carry != 0 {
            bail!("decimal u256 overflow");
        }
    }

    Ok(bytes)
}

fn parse_u256_hex(hex: &str) -> Result<[u8; 32]> {
    if hex.len() > 64 {
        bail!("hex u256 overflow");
    }

    let mut bytes = [0_u8; 32];
    let mut byte_index = 31_usize;
    let mut low_nibble = true;
    for nibble in hex.bytes().rev() {
        let value = match nibble {
            b'0'..=b'9' => nibble - b'0',
            b'a'..=b'f' => nibble - b'a' + 10,
            b'A'..=b'F' => nibble - b'A' + 10,
            _ => bail!("invalid hex u256"),
        };

        if low_nibble {
            bytes[byte_index] = value;
        } else {
            bytes[byte_index] |= value << 4;
            byte_index = byte_index.saturating_sub(1);
        }
        low_nibble = !low_nibble;
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_u256_decimal_and_hex() {
        let value = parse_u256_text("256").unwrap();
        assert_eq!(value[30], 1);
        assert_eq!(value[31], 0);

        let value = parse_u256_text("0x0102").unwrap();
        assert_eq!(value[30], 1);
        assert_eq!(value[31], 2);
    }
}
