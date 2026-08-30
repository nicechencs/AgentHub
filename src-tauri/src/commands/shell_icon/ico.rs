//! Encode a square RGBA bitmap as a 32-bit ICO (for Windows shortcut icons).

pub fn encode_ico_rgba(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    if width != height {
        return Err("ico must be square".into());
    }
    if width == 0 || width > 256 {
        return Err(format!("ico size {width} out of range"));
    }
    let px = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| "ico size overflow".to_string())?;
    let expected = px.checked_mul(4).ok_or_else(|| "ico size overflow".to_string())?;
    if rgba.len() != expected {
        return Err(format!("ico rgba length {} != {expected}", rgba.len()));
    }

    let xor_size = expected;
    let and_row = ((width as usize + 31) / 32) * 4;
    let and_size = and_row
        .checked_mul(height as usize)
        .ok_or_else(|| "ico mask overflow".to_string())?;
    let dib_size = 40usize
        .checked_add(xor_size)
        .and_then(|n| n.checked_add(and_size))
        .ok_or_else(|| "ico dib overflow".to_string())?;

    let mut out = Vec::with_capacity(22 + dib_size);
    // ICONDIR
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    // ICONDIRENTRY
    out.push(if width == 256 { 0 } else { width as u8 });
    out.push(if height == 256 { 0 } else { height as u8 });
    out.push(0);
    out.push(0);
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&(dib_size as u32).to_le_bytes());
    out.extend_from_slice(&22u32.to_le_bytes());
    // BITMAPINFOHEADER (height * 2 includes AND mask)
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&(height * 2).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(xor_size as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    // XOR: BGRA, bottom-up
    let w = width as usize;
    let h = height as usize;
    for y in (0..h).rev() {
        let row = y * w * 4;
        for x in 0..w {
            let i = row + x * 4;
            out.push(rgba[i + 2]);
            out.push(rgba[i + 1]);
            out.push(rgba[i]);
            out.push(rgba[i + 3]);
        }
    }
    out.extend(std::iter::repeat(0u8).take(and_size));
    Ok(out)
}
